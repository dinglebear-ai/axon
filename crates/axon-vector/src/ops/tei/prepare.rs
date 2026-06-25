use super::{EmbedProgress, EmbedSummary, PreparedDoc, StructuredPayload};
use crate::ops::input::classify::path_extension;
use crate::ops::input::select;
use crate::ops::qdrant::env_usize_clamped;
use crate::ops::{SourceDocument, SourceOrigin, prepare_source_document};
use axon_core::config::Config;
use axon_core::content::{is_excluded_url_path, to_markdown};
use axon_core::http::{fetch_html, http_client};
use axon_core::logging::log_warn;
use axon_core::structured::extract_all;
use axon_core::ui::{accent, symbol_for_status};
use futures_util::{StreamExt, stream};
use spider::url::Url;
use std::error::Error;
use std::path::{Path, PathBuf};

mod chunk_guard;
mod input;

use chunk_guard::{
    ChunkVolumeGuardReport, ChunkVolumeLimits, chunk_volume_limits_from_env,
    enforce_chunk_volume_limits_with_report,
};
#[cfg(test)]
use chunk_guard::{chunk_volume_limits_from_values, enforce_chunk_volume_limits};
#[cfg(test)]
use input::read_inputs_with_max_bytes;
use input::{InputRecord, read_inputs};

/// Input record: (source_url_or_path, content, structured_blob).
/// `structured_blob` is populated from the crawl manifest when input is a
/// crawl-output directory (bead axon_rust-jej7.2). Always `None` for single
/// files or raw-text inputs — those don't carry a crawl manifest.
struct PreparedEmbedDocOutcome {
    doc: Option<PreparedDoc>,
    skipped_empty: bool,
    guard_report: ChunkVolumeGuardReport,
}

#[derive(Clone)]
struct PrepareRecordContext {
    input_path: PathBuf,
    input_is_dir: bool,
    input_exists: bool,
    exclude_prefixes: Vec<String>,
    resolved_source_type: String,
    remote_structured: Option<StructuredPayload>,
    chunk_volume_limits: ChunkVolumeLimits,
}

impl ChunkVolumeGuardReport {
    fn add(&mut self, other: Self) {
        self.docs_deduped += other.docs_deduped;
        self.docs_capped += other.docs_capped;
        self.duplicate_chunks_removed += other.duplicate_chunks_removed;
        self.chunks_removed_by_cap += other.chunks_removed_by_cap;
    }

    fn changed(self) -> bool {
        self.docs_deduped > 0
            || self.docs_capped > 0
            || self.duplicate_chunks_removed > 0
            || self.chunks_removed_by_cap > 0
    }
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
    let input_exists = Path::new(input).exists();
    let input_path = PathBuf::from(input);
    let prep_concurrency = env_usize_clamped(
        "AXON_EMBED_PREP_CONCURRENCY",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8)
            .clamp(2, 16),
        1,
        64,
    );
    let chunk_volume_limits = chunk_volume_limits_from_env();
    let mut prepared = Vec::new();
    // Count docs dropped for having no embeddable content so the skip is
    // attributable — without a trace, a directory of empty/whitespace files
    // "succeeds" with an unexplained lower doc count.
    let mut skipped_empty = 0usize;
    let mut guard_report = ChunkVolumeGuardReport::default();
    let record_context = PrepareRecordContext {
        input_path,
        input_is_dir,
        input_exists,
        exclude_prefixes: exclude_prefixes.to_vec(),
        resolved_source_type: resolved_source_type.to_string(),
        remote_structured,
        chunk_volume_limits,
    };
    let mut outcomes = stream::iter(docs)
        .map(|record| {
            let context = record_context.clone();
            prepare_embed_doc_record(record, context)
        })
        .buffer_unordered(prep_concurrency);
    while let Some(outcome) = outcomes.next().await {
        let outcome = outcome.map_err(|err| -> Box<dyn Error> { err.into() })?;
        if outcome.skipped_empty {
            skipped_empty += 1;
        }
        guard_report.add(outcome.guard_report);
        if let Some(doc) = outcome.doc {
            prepared.push(doc);
        }
    }
    if skipped_empty > 0 {
        log_warn(&format!(
            "command=embed skipped_empty_docs count={skipped_empty} (empty or chunked to nothing)"
        ));
    }
    if guard_report.changed() {
        log_warn(&format!(
            "command=embed chunk_volume_guard_summary docs_deduped={} docs_capped={} duplicate_chunks_removed={} chunks_removed_by_cap={}",
            guard_report.docs_deduped,
            guard_report.docs_capped,
            guard_report.duplicate_chunks_removed,
            guard_report.chunks_removed_by_cap
        ));
    }
    Ok(prepared)
}

async fn prepare_embed_doc_record(
    (url, raw, manifest_structured): InputRecord,
    context: PrepareRecordContext,
) -> Result<PreparedEmbedDocOutcome, String> {
    if raw.trim().is_empty() {
        tracing::debug!(url = %url, "embed: skipping empty doc");
        return Ok(PreparedEmbedDocOutcome {
            doc: None,
            skipped_empty: true,
            guard_report: ChunkVolumeGuardReport::default(),
        });
    }
    if context.input_is_dir
        && url.starts_with("http")
        && is_excluded_url_path(&url, &context.exclude_prefixes)
    {
        return Ok(PreparedEmbedDocOutcome {
            doc: None,
            skipped_empty: false,
            guard_report: ChunkVolumeGuardReport::default(),
        });
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
        .or(context.remote_structured);
    let source_doc = if url.starts_with("http://") || url.starts_with("https://") {
        if context.input_is_dir {
            SourceDocument::try_new_crawl_manifest(url, raw, None, structured)
        } else {
            SourceDocument::try_new_web_markdown(
                url,
                raw,
                context.resolved_source_type,
                None,
                None,
                None,
                structured,
            )
        }
    } else if context.input_exists {
        let locator_path = local_locator_path(&context.input_path, &url, context.input_is_dir);
        let ext = path_extension(&locator_path).to_ascii_lowercase();
        if select::should_chunk_as_code(&url) {
            SourceDocument::try_new_file(
                SourceOrigin::LocalFile,
                url,
                locator_path.clone(),
                ext,
                raw,
                context.resolved_source_type,
                Some(locator_path),
                None,
            )
        } else {
            Ok(SourceDocument::new_local_markdown(
                url,
                "local".to_string(),
                raw,
                context.resolved_source_type,
                Some(locator_path),
                None,
            ))
        }
    } else {
        Ok(SourceDocument::new_plain_text(
            url,
            domain,
            raw,
            context.resolved_source_type,
            None,
            None,
        ))
    }?;
    let doc = prepare_source_document(source_doc).await?;
    if doc.chunks.is_empty() {
        tracing::debug!(url = %doc.url, "embed: skipping doc that chunked to nothing");
        return Ok(PreparedEmbedDocOutcome {
            doc: None,
            skipped_empty: true,
            guard_report: ChunkVolumeGuardReport::default(),
        });
    }
    let doc_url = doc.url.clone();
    let guarded = enforce_chunk_volume_limits_with_report(doc, context.chunk_volume_limits);
    let Some(doc) = guarded.doc else {
        tracing::debug!(url = %doc_url, "embed: skipping doc after chunk volume guard");
        return Ok(PreparedEmbedDocOutcome {
            doc: None,
            skipped_empty: true,
            guard_report: guarded.report,
        });
    };
    Ok(PreparedEmbedDocOutcome {
        doc: Some(doc),
        skipped_empty: false,
        guard_report: guarded.report,
    })
}

fn local_locator_path(input_path: &Path, url: &str, input_is_dir: bool) -> String {
    let path = Path::new(url);
    if input_is_dir {
        path.strip_prefix(input_path)
            .ok()
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .filter(|rel| !rel.is_empty())
            .unwrap_or_else(|| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(url)
                    .to_string()
            })
    } else {
        url.to_string()
    }
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
