use super::{
    EmbedProgress, EmbedSummary, PreparedDoc, StructuredPayload,
    tei_manifest::read_manifest_url_map,
};
use crate::core::config::Config;
use crate::core::content::{is_excluded_url_path, to_markdown};
use crate::core::http::{fetch_html, http_client};
use crate::core::logging::log_warn;
use crate::core::structured::extract_all;
use crate::core::ui::{accent, symbol_for_status};
use crate::vector::ops::file_ingest::{chunk_file, chunking_method};
use crate::vector::ops::input;
use crate::vector::ops::input::classify::{
    classify_file_type, is_test_path, language_name, path_extension,
};
use crate::vector::ops::input::code::code_symbol_extraction_status;
use crate::vector::ops::input::select;
use spider::url::Url;
use std::error::Error;
use std::path::{Path, PathBuf};

/// Input record: (source_url_or_path, content, structured_blob).
/// `structured_blob` is populated from the crawl manifest when input is a
/// crawl-output directory (bead axon_rust-jej7.2). Always `None` for single
/// files or raw-text inputs — those don't carry a crawl manifest.
type InputRecord = (String, String, Option<serde_json::Value>);

/// Per-file size ceiling for local embeds, matching the server validator's
/// `mcp_embed_max_local_bytes` default. A multi-megabyte machine-generated
/// file (a 31 MB JSON index, say) grinds the prose chunker for minutes and
/// floods the collection with thousands of junk chunks — directory walks skip
/// such files with a warning; an explicitly named file is a hard error so the
/// user learns the cap instead of silently embedding nothing.
const MAX_LOCAL_EMBED_FILE_BYTES: u64 = 10 * 1024 * 1024;

async fn read_inputs(input: &str) -> Result<Vec<InputRecord>, Box<dyn Error>> {
    read_inputs_with_max_bytes(input, MAX_LOCAL_EMBED_FILE_BYTES).await
}

async fn read_inputs_with_max_bytes(
    input: &str,
    max_file_bytes: u64,
) -> Result<Vec<InputRecord>, Box<dyn Error>> {
    let path = PathBuf::from(input);
    // POSIX-style symlink policy (like `du` / `find -H` / `chown -H`): a
    // symlink named explicitly on the command line is followed —
    // `tokio::fs::metadata` resolves it — while symlinks *encountered during
    // traversal* are skipped (see collect_embed_files). The server path is
    // stricter and rejects a symlinked root outright (services/embed.rs).
    match tokio::fs::metadata(&path).await {
        Ok(meta) if meta.is_file() => {
            if meta.len() > max_file_bytes {
                return Err(format!(
                    "embed input {input} is {} bytes, over the {max_file_bytes}-byte local file cap",
                    meta.len()
                )
                .into());
            }
            Ok(vec![(
                path.to_string_lossy().to_string(),
                tokio::fs::read_to_string(&path).await?,
                None,
            )])
        }
        Ok(meta) if meta.is_dir() => {
            let manifest_urls = read_manifest_url_map(&path);
            let files = collect_embed_files(&path).await?;
            let mut out = Vec::new();
            for p in files {
                // Oversized files are skipped (not fatal) on directory walks —
                // a single machine-generated multi-MB file must not grind the
                // chunker or sink an otherwise-fine embed.
                match tokio::fs::metadata(&p).await {
                    Ok(meta) if meta.len() > max_file_bytes => {
                        log_warn(&format!(
                            "command=embed skip_oversized_file path={} size_bytes={} cap_bytes={max_file_bytes}",
                            p.display(),
                            meta.len()
                        ));
                        continue;
                    }
                    _ => {}
                }
                // A canonicalize failure silently defeats the manifest lookup
                // (keyed on the canonical path): the file would re-embed even
                // when unchanged and lose its crawl URL / structured payload.
                // Leave a trace so a manifest-skip miss is attributable.
                let canonical = std::fs::canonicalize(&p).unwrap_or_else(|e| {
                    log_warn(&format!(
                        "command=embed canonicalize_failed path={} err={e}",
                        p.display()
                    ));
                    p.clone()
                });
                let (source, changed, structured) = manifest_urls
                    .get(&canonical)
                    .map(|(u, c, s)| (u.clone(), *c, s.clone()))
                    .unwrap_or_else(|| (p.to_string_lossy().to_string(), true, None));
                if !changed {
                    continue;
                }
                // Skip-on-error: a single unreadable/non-UTF-8 file (one that
                // slipped the binary-extension filter) must not fail the whole
                // embed job — log and move on.
                match tokio::fs::read_to_string(&p).await {
                    Ok(content) => out.push((source, content, structured)),
                    Err(e) => {
                        log_warn(&format!(
                            "command=embed skip_unreadable_file path={} err={e}",
                            p.display()
                        ));
                    }
                }
            }
            Ok(out)
        }
        // Path-shaped input that doesn't resolve is a hard error, never
        // free-text: an embed job for a host path claimed by a worker in a
        // different filesystem namespace (the axon container cannot see
        // /home/<user>/...) used to fall through here and "successfully"
        // embed the literal path string as a one-chunk document.
        _ if select::looks_path_like(input) => Err(format!(
            "local embed path does not exist or is not visible to this process: {input} \
             (local paths must be embedded by a process on the same filesystem — \
             run `axon embed {input}` on the machine that owns the path)"
        )
        .into()),
        _ => Ok(vec![(input.to_string(), input.to_string(), None)]),
    }
}

/// Recursively collect embeddable files under `root`, descending into
/// subdirectories. Prunes VCS/dependency/build directories (`select::is_pruned_dir`)
/// and skips known-binary file extensions (`select::is_binary_ext`) before any
/// read. Symlinks are skipped (their `file_type` is neither file nor dir). The
/// returned paths are sorted for deterministic embed order.
///
/// Resilience matches the file read: the **top-level** `root` read is a hard
/// error (nothing to embed if the target is unreadable), but an unreadable
/// **subdirectory** is logged and skipped rather than failing the whole embed —
/// so one permission-protected subtree doesn't sink an otherwise-fine job.
async fn collect_embed_files(root: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut at_root = true;
    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(e) if at_root => {
                return Err(format!("invalid embed directory {}: {e}", dir.display()).into());
            }
            Err(e) => {
                log_warn(&format!(
                    "command=embed skip_unreadable_dir path={} err={e}",
                    dir.display()
                ));
                at_root = false;
                continue;
            }
        };
        at_root = false;
        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(e) => {
                    log_warn(&format!(
                        "command=embed dir_iter_error path={} err={e}",
                        dir.display()
                    ));
                    break;
                }
            };
            let p = entry.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let Ok(file_type) = entry.file_type().await else {
                log_warn(&format!(
                    "command=embed skip_unknown_type path={}",
                    p.display()
                ));
                continue;
            };
            if file_type.is_dir() {
                if !select::is_pruned_dir(name) {
                    stack.push(p);
                }
            } else if file_type.is_file() && !select::is_binary_ext(path_extension(name)) {
                files.push(p);
            }
        }
    }
    files.sort();
    Ok(files)
}

pub(super) async fn prepare_embed_docs(
    cfg: &Config,
    input: &str,
    exclude_prefixes: &[String],
    source_type: Option<&str>,
) -> Result<Vec<PreparedDoc>, Box<dyn Error>> {
    let resolved_source_type = source_type.unwrap_or("embed");
    let mut docs = read_inputs(input).await?;
    // When fetching a remote URL, run the structured-data pass on the raw
    // HTML before converting to markdown so we can attach JSON-LD /
    // __NEXT_DATA__ / SvelteKit payloads to every chunk. Local file / dir
    // inputs do not carry HTML — structured stays `None` for those (crawl
    // path writes structured blobs into the manifest instead, so the crawl
    // case is handled below via `manifest_structured`).
    let mut remote_structured: Option<StructuredPayload> = None;
    if docs.len() == 1 && !Path::new(input).exists() && input.starts_with("http") {
        let client = http_client()?.clone();
        let html = fetch_html(&client, input).await?;
        let pass = extract_all(&html);
        if !pass.is_empty() {
            remote_structured = StructuredPayload::from_pass(&pass, cfg.structured_data_max_bytes);
            if let Some(ref sp) = remote_structured {
                tracing::debug!(
                    url = %input,
                    kind = sp.kind,
                    schema_type = ?sp.schema_type,
                    blob_bytes = serde_json::to_string(&sp.blob).map_or(0, |s| s.len()),
                    "structured.extracted"
                );
            }
        }
        docs = vec![(input.to_string(), to_markdown(&html, None), None)];
    }
    let input_is_dir = Path::new(input).is_dir();
    let mut prepared = Vec::new();
    // Count docs dropped for having no embeddable content so the skip is
    // attributable — without a trace, a directory of empty/whitespace files
    // "succeeds" with an unexplained lower doc count.
    let mut skipped_empty = 0usize;
    for (url, raw, manifest_structured) in docs {
        if raw.trim().is_empty() {
            tracing::debug!(url = %url, "embed: skipping empty doc");
            skipped_empty += 1;
            continue;
        }
        if input_is_dir && url.starts_with("http") && is_excluded_url_path(&url, exclude_prefixes) {
            continue;
        }
        // For local code files: emit one PreparedDoc per CodeChunk with
        // canonical code_* / symbol_* extra payload (mirrors the ingest path).
        // Crawl-manifest docs (http URL) and prose files stay on the existing
        // select_chunks prose path below.
        if !url.starts_with("http") && select::should_chunk_as_code(&url) {
            match embed_code_file_docs(&url, raw, resolved_source_type).await {
                Some(docs) => prepared.extend(docs),
                None => skipped_empty += 1,
            }
            continue;
        }
        let (chunks, content_type) = select_chunks(&url, raw).await;
        if chunks.is_empty() {
            tracing::debug!(url = %url, "embed: skipping doc that chunked to nothing");
            skipped_empty += 1;
            continue;
        }
        let domain = Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown".to_string());
        // Reconstruct StructuredPayload from the manifest blob written by
        // process_page() at crawl time (bead axon_rust-jej7.2). Falls back
        // to the remote-URL path's StructuredPayload when no manifest blob is
        // present (e.g. single-URL embed, plain file embed).
        let structured = manifest_structured
            .as_ref()
            .and_then(structured_payload_from_blob)
            .or(remote_structured.clone());
        prepared.push(PreparedDoc {
            url,
            domain,
            chunks,
            source_type: resolved_source_type.to_string(),
            content_type,
            title: None,
            extra: None,
            extractor_name: None,
            structured,
            chunk_extra: Vec::new(),
        });
    }
    if skipped_empty > 0 {
        log_warn(&format!(
            "command=embed skipped_empty_docs count={skipped_empty} (empty or chunked to nothing)"
        ));
    }
    Ok(prepared)
}

/// Chunk a local code file and build one `PreparedDoc` per `CodeChunk`, each
/// carrying canonical `code_*`/`symbol_*` extra payload.
///
/// Returns `Some(docs)` on success (may be empty if all chunks are empty),
/// or `None` when the file chunked to nothing (caller should count as skipped).
async fn embed_code_file_docs(
    url: &str,
    raw: String,
    source_type: &str,
) -> Option<Vec<PreparedDoc>> {
    let ext = path_extension(url).to_ascii_lowercase();
    let raw_c = raw.clone();
    let ext_c = ext.clone();
    let code_chunks = match tokio::task::spawn_blocking(move || chunk_file(&raw_c, &ext_c)).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(url = %url, err = %e, "embed: chunk_file task panicked");
            return None;
        }
    };
    if code_chunks.is_empty() {
        tracing::debug!(url = %url, "embed: local code file chunked to nothing");
        return None;
    }
    let symbol_status = code_symbol_extraction_status(&raw, &ext, &code_chunks);
    let domain = if url.starts_with("http://") || url.starts_with("https://") {
        Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "local".to_string())
    } else {
        // Local file path — use parent directory name as domain identifier
        Path::new(url)
            .components()
            .rev()
            .nth(1) // parent dir
            .and_then(|c| c.as_os_str().to_str())
            .unwrap_or("local")
            .to_string()
    };
    let mut docs = Vec::with_capacity(code_chunks.len());
    for (idx, chunk) in code_chunks.into_iter().enumerate() {
        let method = chunking_method(&ext, &chunk);
        let extra = serde_json::json!({
            "code_file_path": url,
            "code_language": language_name(&ext),
            "code_file_type": classify_file_type(url),
            "code_is_test": is_test_path(url),
            "code_line_start": chunk.start_line,
            "code_line_end": chunk.end_line,
            "code_chunking_method": method,
            "symbol_name": chunk.symbol_name(),
            "symbol_kind": chunk.symbol_kind_str(),
            "symbol_extraction_status": symbol_status,
        });
        docs.push(PreparedDoc {
            url: format!("{url}#L{}-L{}#{}", chunk.start_line, chunk.end_line, idx),
            domain: domain.clone(),
            chunks: vec![chunk.text],
            source_type: source_type.to_string(),
            content_type: "text",
            title: Some(url.to_string()),
            extra: Some(extra),
            extractor_name: None,
            structured: None,
            chunk_extra: Vec::new(),
        });
    }
    Some(docs)
}

/// Choose a chunking strategy for one document and report its content type.
///
/// - **Code** (local source files with a tree-sitter grammar): AST-aware
///   `chunk_code`, run on a blocking thread because tree-sitter parsing is
///   CPU-bound and would otherwise stall the embed worker's tokio runtime.
///   Falls back to `chunk_text` for grammars that fail. Tagged `"text"`.
/// - **Prose with control chars**: `chunk_text` — `MarkdownSplitter` panics on
///   embedded control characters, so this guard is preserved. Tagged `"markdown"`.
/// - **Prose / docs** (default): `chunk_markdown`. Tagged `"markdown"`.
///
/// Code chunking applies only to local paths (`url` not `http`-prefixed) — crawl
/// output and remote single-doc embeds carry an http `url` and stay on the prose
/// path, so this is safe for the crawl-output reuse of this function.
async fn select_chunks(url: &str, raw: String) -> (Vec<String>, &'static str) {
    if !url.starts_with("http") && select::should_chunk_as_code(url) {
        let ext = path_extension(url).to_ascii_lowercase();
        // Match the JoinError explicitly: a tree-sitter panic inside
        // spawn_blocking must not be silently collapsed to an empty Vec (which
        // would drop the doc downstream without any trace). Log and drop
        // deliberately, mirroring the skip-on-error file/dir paths above.
        let chunks = match tokio::task::spawn_blocking(move || {
            input::code::chunk_code(&raw, &ext).unwrap_or_else(|| input::chunk_text(&raw))
        })
        .await
        {
            Ok(chunks) => chunks,
            Err(e) => {
                log_warn(&format!(
                    "command=embed code_chunk_join_error url={url} err={e}"
                ));
                Vec::new()
            }
        };
        return (chunks, "text");
    }
    // Fall back to chunk_text for inputs containing control characters
    // (e.g. binary or non-markdown data) — MarkdownSplitter can panic on them.
    let chunks = if raw
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        input::chunk_text(&raw)
    } else {
        input::chunk_markdown(&raw)
    };
    (chunks, "markdown")
}

/// Reconstruct a `StructuredPayload` from the JSON blob stored in the crawl
/// manifest (bead axon_rust-jej7.2). The blob is the JSON object produced by
/// `extract_structured_blob()` in `collector/page.rs`:
///   `{ "kind": "jsonld" | "next_data" | "sveltekit",
///      "blob": <raw JSON value>,
///      "schema_type"?: "Article" | ...,
///      "schema_id"?: "https://..." }`
///
/// Returns `None` when `kind` is missing or not a known static string.
fn structured_payload_from_blob(blob: &serde_json::Value) -> Option<StructuredPayload> {
    let kind: &'static str = match blob.get("kind").and_then(|v| v.as_str())? {
        "jsonld" => "jsonld",
        "next_data" => "next_data",
        "sveltekit" => "sveltekit",
        _ => return None,
    };
    let inner_blob = blob.get("blob")?.clone();
    let schema_type = blob
        .get("schema_type")
        .and_then(|v| v.as_str())
        .map(String::from);
    let schema_id = blob
        .get("schema_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    Some(StructuredPayload {
        kind,
        schema_type,
        schema_id,
        blob: inner_blob,
    })
}

pub(super) fn emit_empty_embed(
    progress_tx: Option<tokio::sync::mpsc::Sender<EmbedProgress>>,
) -> Result<EmbedSummary, Box<dyn Error>> {
    if let Some(tx) = &progress_tx {
        let _ = tx.try_send(EmbedProgress {
            docs_total: 0,
            docs_completed: 0,
            chunks_embedded: 0,
        });
    }
    Ok(EmbedSummary {
        docs_embedded: 0,
        docs_failed: 0,
        chunks_embedded: 0,
    })
}

pub(super) fn emit_embed_summary(cfg: &Config, docs_embedded: usize, chunks_embedded: usize) {
    if cfg.json_output {
        return;
    }
    eprintln!(
        "{} embedded {} chunks from {} docs into {}",
        symbol_for_status("completed"),
        chunks_embedded,
        docs_embedded,
        accent(&cfg.collection)
    );
}

#[cfg(test)]
#[path = "prepare_tests.rs"]
mod tests;
