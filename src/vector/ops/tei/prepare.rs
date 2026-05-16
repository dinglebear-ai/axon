use super::{
    EmbedProgress, EmbedSummary, PreparedDoc, StructuredPayload,
    tei_manifest::read_manifest_url_map,
};
use crate::core::config::Config;
use crate::core::content::{is_excluded_url_path, to_markdown};
use crate::core::http::{fetch_html, http_client};
use crate::core::structured::extract_all;
use crate::core::ui::{accent, symbol_for_status};
use crate::vector::ops::input;
use spider::url::Url;
use std::error::Error;
use std::path::{Path, PathBuf};

/// Input record: (source_url_or_path, content, structured_blob).
/// `structured_blob` is populated from the crawl manifest when input is a
/// crawl-output directory (bead axon_rust-jej7.2). Always `None` for single
/// files or raw-text inputs — those don't carry a crawl manifest.
type InputRecord = (String, String, Option<serde_json::Value>);

async fn read_inputs(input: &str) -> Result<Vec<InputRecord>, Box<dyn Error>> {
    let path = PathBuf::from(input);
    match tokio::fs::metadata(&path).await {
        Ok(meta) if meta.is_file() => Ok(vec![(
            path.to_string_lossy().to_string(),
            tokio::fs::read_to_string(&path).await?,
            None,
        )]),
        Ok(meta) if meta.is_dir() => {
            let manifest_urls = read_manifest_url_map(&path);
            let mut dir = tokio::fs::read_dir(&path).await?;
            let mut files = Vec::new();
            while let Some(entry) = dir.next_entry().await? {
                let p = entry.path();
                if tokio::fs::metadata(&p).await.is_ok_and(|m| m.is_file()) {
                    files.push(p);
                }
            }
            files.sort();
            let mut out = Vec::new();
            for p in files {
                let canonical = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
                let (source, changed, structured) = manifest_urls
                    .get(&canonical)
                    .map(|(u, c, s)| (u.clone(), *c, s.clone()))
                    .unwrap_or_else(|| (p.to_string_lossy().to_string(), true, None));
                if !changed {
                    continue;
                }
                let content = tokio::fs::read_to_string(&p).await?;
                out.push((source, content, structured));
            }
            Ok(out)
        }
        _ => Ok(vec![(input.to_string(), input.to_string(), None)]),
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
    let mut prepared = Vec::new();
    for (url, raw, manifest_structured) in docs {
        if raw.trim().is_empty() {
            continue;
        }
        if input_is_dir && url.starts_with("http") && is_excluded_url_path(&url, exclude_prefixes) {
            continue;
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
            .and_then(|blob| structured_payload_from_blob(blob))
            .or_else(|| remote_structured.clone());
        prepared.push(PreparedDoc {
            url,
            domain,
            chunks,
            source_type: resolved_source_type.to_string(),
            content_type: "markdown",
            title: None,
            extra: None,
            extractor_name: None,
            structured,
        });
    }
    Ok(prepared)
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
mod tests {
    use super::prepare_embed_docs;
    use crate::core::config::Config;
    use tempfile::TempDir;

    #[tokio::test]
    async fn prepare_embed_docs_uses_given_source_type() {
        let cfg = Config::default_lite();
        let temp_dir = TempDir::new().expect("tempdir");
        let input_path = temp_dir.path().join("doc.md");
        tokio::fs::write(&input_path, "# Crawl doc\n\nhello there")
            .await
            .expect("write markdown fixture");

        let prepared = prepare_embed_docs(&cfg, &input_path.to_string_lossy(), &[], Some("crawl"))
            .await
            .expect("prepare docs");

        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].source_type, "crawl");
    }

    #[tokio::test]
    async fn prepare_embed_docs_defaults_to_embed() {
        let cfg = Config::default_lite();
        let temp_dir = TempDir::new().expect("tempdir");
        let input_path = temp_dir.path().join("doc.md");
        tokio::fs::write(&input_path, "# Embed doc\n\nthis is a test")
            .await
            .expect("write markdown fixture");

        let prepared = prepare_embed_docs(&cfg, &input_path.to_string_lossy(), &[], None)
            .await
            .expect("prepare docs");

        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].source_type, "embed");
    }
}
