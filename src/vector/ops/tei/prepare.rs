use super::{EmbedProgress, EmbedSummary, PreparedDoc, tei_manifest::read_manifest_url_map};
use crate::core::content::{is_excluded_url_path, to_markdown};
use crate::core::http::{fetch_html, http_client};
use crate::core::ui::{accent, symbol_for_status};
use crate::vector::ops::input;
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
    source_type: Option<&str>,
) -> Result<Vec<PreparedDoc>, Box<dyn Error>> {
    let resolved_source_type = source_type.unwrap_or("embed");
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
        prepared.push(PreparedDoc {
            url,
            domain,
            chunks,
            source_type: resolved_source_type.to_string(),
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
        docs_failed: 0,
        chunks_embedded: 0,
    })
}

pub(super) fn emit_embed_summary(
    cfg: &crate::core::config::Config,
    docs_embedded: usize,
    chunks_embedded: usize,
) {
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
    use tempfile::TempDir;

    #[tokio::test]
    async fn prepare_embed_docs_uses_given_source_type() {
        let temp_dir = TempDir::new().expect("tempdir");
        let input_path = temp_dir.path().join("doc.md");
        tokio::fs::write(&input_path, "# Crawl doc\n\nhello there")
            .await
            .expect("write markdown fixture");

        let prepared = prepare_embed_docs(&input_path.to_string_lossy(), &[], Some("crawl"))
            .await
            .expect("prepare docs");

        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].source_type, "crawl");
    }

    #[tokio::test]
    async fn prepare_embed_docs_defaults_to_embed() {
        let temp_dir = TempDir::new().expect("tempdir");
        let input_path = temp_dir.path().join("doc.md");
        tokio::fs::write(&input_path, "# Embed doc\n\nthis is a test")
            .await
            .expect("write markdown fixture");

        let prepared = prepare_embed_docs(&input_path.to_string_lossy(), &[], None)
            .await
            .expect("prepare docs");

        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].source_type, "embed");
    }
}
