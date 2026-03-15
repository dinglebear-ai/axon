use super::state::upsert_target_state;
use super::{RefreshPageResult, RefreshRunSummary, RefreshTargetState};
use crate::crates::core::config::Config;
use crate::crates::core::content::url_to_filename;
use crate::crates::core::logging::log_warn;
use crate::crates::crawl::manifest::ManifestEntry;
use crate::crates::vector::ops::input::chunk_text;
use crate::crates::vector::ops::tei::{PreparedDoc, embed_prepared_docs};
use sqlx::PgPool;
use std::path::Path;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use super::processor::refresh_one_url;

/// Context for processing refresh URLs, reducing parameter count.
pub(crate) struct RefreshUrlContext<'a> {
    pub cfg: &'a Config,
    pub pool: &'a PgPool,
    pub client: &'a reqwest::Client,
    pub markdown_dir: &'a Path,
    pub manifest: &'a mut tokio::io::BufWriter<tokio::fs::File>,
    pub job_id: Uuid,
    pub embed: bool,
}

/// Validate that `output_dir` does not escape `base_dir` via path traversal.
///
/// Uses `tokio::fs::canonicalize` (non-blocking) when paths exist on disk. For
/// non-existent paths, falls back to manual component-level normalization which
/// catches traversal attempts like `/base/../../../etc` even when the path does
/// not yet exist on disk.
/// Make a path absolute by prepending CWD if it is relative.
fn make_absolute(p: &Path) -> std::path::PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}

pub(crate) async fn validate_output_dir(
    output_dir: &Path,
    base_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let canonical_output = tokio::fs::canonicalize(output_dir)
        .await
        .unwrap_or_else(|_| normalize_path(&make_absolute(output_dir)));
    let canonical_base = tokio::fs::canonicalize(base_dir)
        .await
        .unwrap_or_else(|_| normalize_path(&make_absolute(base_dir)));
    if !canonical_output.starts_with(&canonical_base) {
        return Err(format!(
            "output_dir path traversal rejected: {} is outside base {}",
            canonical_output.display(),
            canonical_base.display()
        )
        .into());
    }
    Ok(())
}

/// Normalize a path by resolving `.` and `..` components without touching the filesystem.
fn normalize_path(path: &Path) -> std::path::PathBuf {
    use std::path::Component;
    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other),
        }
    }
    normalized
}

/// Process a single URL within a refresh job: fetch, hash-compare, write markdown, embed.
pub(crate) async fn process_single_refresh_url(
    ctx: &mut RefreshUrlContext<'_>,
    url: &str,
    previous: Option<&RefreshTargetState>,
    summary: &mut RefreshRunSummary,
    changed_idx: &mut u32,
) {
    match refresh_one_url(ctx.client, url, previous).await {
        Ok(result) => {
            if result.status_code >= 400 {
                summary.failed += 1;
                let error_text = format!("HTTP {}", result.status_code);
                if let Err(e) = upsert_target_state(ctx.pool, url, &result, Some(&error_text)).await
                {
                    log_warn(&format!("state_upsert_failed url={url} error={e}"));
                }
                return;
            }

            if result.not_modified {
                summary.not_modified += 1;
                summary.unchanged += 1;
                if let Err(e) = upsert_target_state(ctx.pool, url, &result, None).await {
                    log_warn(&format!("state_upsert_failed url={url} error={e}"));
                }
                return;
            }

            if result.changed {
                *changed_idx += 1;
                summary.changed += 1;

                if let Some(markdown) = result.markdown.as_deref() {
                    let filename = url_to_filename(url, *changed_idx);
                    let file_path = ctx.markdown_dir.join(&filename);
                    if let Err(err) = tokio::fs::write(&file_path, markdown.as_bytes()).await {
                        summary.failed += 1;
                        if let Err(e) = upsert_target_state(
                            ctx.pool,
                            url,
                            &result,
                            Some(&format!("write markdown failed: {err}")),
                        )
                        .await
                        {
                            log_warn(&format!("state_upsert_failed url={url} error={e}"));
                        }
                        return;
                    }

                    let entry = ManifestEntry {
                        url: url.to_string(),
                        relative_path: format!("markdown/{filename}"),
                        markdown_chars: result.markdown_chars.unwrap_or(0),
                        content_hash: result.content_hash.clone(),
                        changed: true,
                    };
                    if let Ok(mut line) = serde_json::to_string(&entry) {
                        line.push('\n');
                        if let Err(e) = ctx.manifest.write_all(line.as_bytes()).await {
                            log_warn(&format!("manifest_write_failed url={url} error={e}"));
                        }
                    }

                    if ctx.embed {
                        let domain = spider::url::Url::parse(url)
                            .ok()
                            .and_then(|u| u.host_str().map(|s| s.to_string()))
                            .unwrap_or_else(|| "unknown".to_string());
                        let chunks = chunk_text(markdown);
                        if chunks.is_empty() {
                            log_warn(&format!("refresh embed skipped (no chunks) for url={url}"));
                        } else {
                            let doc = PreparedDoc {
                                url: url.to_string(),
                                domain,
                                chunks,
                                source_type: "refresh".to_string(),
                                content_type: "markdown",
                                title: None,
                                extra: None,
                            };
                            match embed_prepared_docs(ctx.cfg, vec![doc], None).await {
                                Ok(result) => {
                                    summary.embedded_chunks += result.chunks_embedded;
                                    if result.docs_failed > 0 {
                                        log_warn(&format!(
                                            "refresh embed partial failure for url={url} \
                                             job_id={}: {}/{} docs failed",
                                            ctx.job_id, result.docs_failed, 1
                                        ));
                                    }
                                }
                                Err(err) => {
                                    log_warn(&format!(
                                        "refresh embed failed for url={} job_id={}: {}",
                                        url, ctx.job_id, err
                                    ));
                                }
                            }
                        }
                    }
                }
            } else {
                summary.unchanged += 1;
            }

            if let Err(e) = upsert_target_state(ctx.pool, url, &result, None).await {
                log_warn(&format!("state_upsert_failed url={url} error={e}"));
            }
        }
        Err(err) => {
            summary.failed += 1;
            let fallback = RefreshPageResult {
                status_code: 0,
                etag: previous.and_then(|s| s.etag.clone()),
                last_modified: previous.and_then(|s| s.last_modified.clone()),
                content_hash: previous.and_then(|s| s.content_hash.clone()),
                markdown_chars: None,
                markdown: None,
                changed: false,
                not_modified: false,
            };
            if let Err(e) =
                upsert_target_state(ctx.pool, url, &fallback, Some(&err.to_string())).await
            {
                log_warn(&format!("state_upsert_failed url={url} error={e}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn validate_output_dir_rejects_traversal() {
        let base = PathBuf::from("/tmp/axon-test");
        let traversal = PathBuf::from("/tmp/axon-test/../../../etc");
        assert!(
            validate_output_dir(&traversal, &base).await.is_err(),
            "path traversal should be rejected"
        );
    }

    #[tokio::test]
    async fn validate_output_dir_accepts_safe_subpath() {
        let base = PathBuf::from("/tmp");
        let safe = PathBuf::from("/tmp/axon-test/output");
        // Both paths may not exist, but the uncanonicalized fallback should accept
        // /tmp/axon-test/output as inside /tmp.
        assert!(
            validate_output_dir(&safe, &base).await.is_ok(),
            "safe subpath should be accepted"
        );
    }

    #[tokio::test]
    async fn refresh_one_url_rejects_private_ips() {
        let client = reqwest::Client::new();
        for bad_url in &[
            "http://192.168.1.1/",
            "http://10.0.0.1/",
            "http://127.0.0.1/",
        ] {
            let result = refresh_one_url(&client, bad_url, None).await;
            assert!(result.is_err(), "expected SSRF rejection for {bad_url}");
        }
    }
}
