use super::{EmbedProgress, EmbedSummary, PreparedDoc, tei_manifest::read_manifest_url_map};
use crate::crates::core::content::{is_excluded_url_path, to_markdown};
use crate::crates::core::http::{fetch_html, http_client};
use crate::crates::core::ui::{accent, symbol_for_status};
use crate::crates::vector::ops::input;
use spider::url::Url;
use std::error::Error;
use std::path::{Path, PathBuf};

async fn read_inputs(input: &str) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let path = PathBuf::from(input);
    match tokio::fs::metadata(&path).await {
        Ok(meta) if meta.is_file() => Ok(vec![(
            path.to_string_lossy().to_string(),
            tokio::fs::read_to_string(&path).await?,
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
                let (source, changed) = manifest_urls
                    .get(&canonical)
                    .map(|(u, c)| (u.clone(), *c))
                    .unwrap_or_else(|| (p.to_string_lossy().to_string(), true));
                if !changed {
                    continue;
                }
                let content = tokio::fs::read_to_string(&p).await?;
                out.push((source, content));
            }
            Ok(out)
        }
        _ => Ok(vec![(input.to_string(), input.to_string())]),
    }
}

pub(super) async fn prepare_embed_docs(
    input: &str,
    exclude_prefixes: &[String],
) -> Result<Vec<PreparedDoc>, Box<dyn Error>> {
    let mut docs = read_inputs(input).await?;
    if docs.len() == 1 && !Path::new(input).exists() && input.starts_with("http") {
        let client = http_client()?.clone();
        let html = fetch_html(&client, input).await?;
        docs = vec![(input.to_string(), to_markdown(&html, None))];
    }
    let input_is_dir = Path::new(input).is_dir();
    let mut prepared = Vec::new();
    for (url, raw) in docs {
        if raw.trim().is_empty() {
            continue;
        }
        if input_is_dir && url.starts_with("http") && is_excluded_url_path(&url, exclude_prefixes) {
            continue;
        }
        let chunks = input::chunk_text(&raw);
        let domain = Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown".to_string());
        prepared.push(PreparedDoc {
            url,
            domain,
            chunks,
            source_type: "embed".to_string(),
            content_type: "markdown",
            title: None,
            extra: None,
        });
    }
    Ok(prepared)
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
        chunks_embedded: 0,
    })
}

pub(super) fn emit_embed_summary(
    cfg: &crate::crates::core::config::Config,
    chunks_embedded: usize,
) {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({"chunks_embedded": chunks_embedded, "collection": cfg.collection})
        );
    } else {
        println!(
            "{} embedded {} chunks into {}",
            symbol_for_status("completed"),
            chunks_embedded,
            accent(&cfg.collection)
        );
    }
}
