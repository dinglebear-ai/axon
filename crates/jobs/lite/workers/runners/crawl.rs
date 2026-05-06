use std::collections::HashMap;
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::Notify;

use super::super::progress::spawn_crawl_progress_persister;
use crate::crates::core::config::Config;
use crate::crates::core::ui::{accent, symbol_for_status};
use crate::crates::jobs::backend::{JobPayload, lift_err};
use crate::crates::jobs::lite::config_snapshot::{
    apply_lite_config_snapshot, lite_config_snapshot_json,
};
use crate::crates::jobs::lite::ops::enqueue_job;

use super::JobResult;

pub async fn run_crawl_job_lite(
    pool: &SqlitePool,
    cfg: &Config,
    id: uuid::Uuid,
    embed_notify: Option<Arc<Notify>>,
) -> JobResult {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT url, config_json FROM axon_crawl_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((url, config_json)) = row else {
        tracing::warn!(id = %id, table = "axon_crawl_jobs", "job row not found at execution time, may have been deleted mid-run");
        return Ok(None);
    };
    let effective_cfg = apply_lite_config_snapshot(cfg, &config_json).map_err(lift_err)?;

    crate::crates::core::http::validate_url(&url).map_err(lift_err)?;

    let job_output_dir = crate::crates::services::crawl::predict_crawl_output_dir(
        &effective_cfg.output_dir,
        &url,
        &id.to_string(),
    );

    let (progress_tx, progress_task) = spawn_crawl_progress_persister(pool, id);
    let (summary, _) = crate::crates::crawl::engine::run_crawl_once(
        &effective_cfg,
        &url,
        effective_cfg.render_mode,
        &job_output_dir,
        Some(progress_tx),
        effective_cfg.discover_sitemaps,
        Arc::new(HashMap::new()),
        Some(&id.to_string()),
    )
    .await
    .map_err(lift_err)?;
    if let Err(e) = progress_task.await {
        tracing::warn!(job_id = %id, error = %e, "crawl progress persister task failed");
    }

    let embed_job_id = if effective_cfg.embed && summary.markdown_files > 0 {
        let markdown_dir = job_output_dir
            .join("markdown")
            .to_string_lossy()
            .to_string();
        match enqueue_job(
            pool,
            &JobPayload::Embed {
                input: markdown_dir,
                config_json: lite_config_snapshot_json(&effective_cfg).map_err(lift_err)?,
            },
        )
        .await
        {
            Ok(eid) => {
                if let Some(notify) = &embed_notify {
                    notify.notify_one();
                }
                Some(eid.to_string())
            }
            Err(e) => {
                tracing::warn!("lite crawl worker: failed to enqueue embed job: {e}");
                None
            }
        }
    } else {
        None
    };

    if !effective_cfg.json_output && !effective_cfg.quiet {
        eprintln!(
            "{} crawl completed {} pages={} markdown={} thin={} errors={} elapsed={} job={} output={}",
            symbol_for_status("completed"),
            accent(&url),
            summary.pages_seen,
            summary.markdown_files,
            summary.thin_pages,
            summary.error_pages,
            format_elapsed_ms(summary.elapsed_ms),
            id,
            job_output_dir.join("markdown").display()
        );
        if let Some(embed_job_id) = &embed_job_id {
            eprintln!("  embed queued job={embed_job_id}");
        } else if effective_cfg.embed {
            eprintln!("  embed skipped no markdown output");
        } else {
            eprintln!("  embed disabled");
        }
    }

    Ok(Some(build_crawl_result_json(
        &url,
        &summary,
        embed_job_id.as_deref(),
    )))
}

/// Builds the canonical result JSON written to `axon_crawl_jobs.result_json`.
/// Key set is locked by `crawl_result_json_canonical_key_set_is_exact`.
fn build_crawl_result_json(
    url: &str,
    summary: &crate::crates::crawl::engine::CrawlSummary,
    embed_job_id: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "url": url,
        "pages_crawled": summary.pages_seen,
        "md_created": summary.markdown_files,
        "pages_discovered": summary.pages_discovered,
        "thin_md": summary.thin_pages,
        "error_pages": summary.error_pages,
        "waf_blocked_pages": summary.waf_blocked_pages,
        "elapsed_ms": summary.elapsed_ms,
        "embed_job_id": embed_job_id,
    })
}

fn format_elapsed_ms(elapsed_ms: u128) -> String {
    if elapsed_ms >= 1_000 {
        format!("{:.1}s", elapsed_ms as f64 / 1_000.0)
    } else {
        format!("{elapsed_ms}ms")
    }
}

#[cfg(test)]
mod tests {
    use super::build_crawl_result_json;
    use crate::crates::crawl::engine::CrawlSummary;

    fn make_summary() -> CrawlSummary {
        CrawlSummary {
            pages_seen: 7,
            markdown_files: 5,
            pages_discovered: 9,
            thin_pages: 2,
            error_pages: 1,
            waf_blocked_pages: 0,
            elapsed_ms: 1234,
            ..CrawlSummary::default()
        }
    }

    #[test]
    fn crawl_result_json_uses_canonical_keys() {
        let json =
            build_crawl_result_json("https://example.com", &make_summary(), Some("embed-job-id"));
        let obj = json.as_object().expect("json is an object");

        // Canonical keys are present
        assert_eq!(
            obj.get("url").and_then(|v| v.as_str()),
            Some("https://example.com")
        );
        assert_eq!(obj.get("pages_crawled").and_then(|v| v.as_u64()), Some(7));
        assert_eq!(obj.get("md_created").and_then(|v| v.as_u64()), Some(5));
        assert_eq!(
            obj.get("pages_discovered").and_then(|v| v.as_u64()),
            Some(9)
        );
        assert_eq!(obj.get("thin_md").and_then(|v| v.as_u64()), Some(2));
        assert_eq!(obj.get("error_pages").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(
            obj.get("waf_blocked_pages").and_then(|v| v.as_u64()),
            Some(0)
        );
        assert_eq!(obj.get("elapsed_ms").and_then(|v| v.as_u64()), Some(1234));
        assert_eq!(
            obj.get("embed_job_id").and_then(|v| v.as_str()),
            Some("embed-job-id")
        );
    }

    #[test]
    fn crawl_result_json_omits_legacy_aliases() {
        let json = build_crawl_result_json("https://example.com", &make_summary(), None);
        let obj = json.as_object().expect("json is an object");

        // Legacy aliases removed (axon_rust-pkl.8)
        assert!(
            !obj.contains_key("pages_seen"),
            "legacy alias pages_seen must not appear in crawl result JSON"
        );
        assert!(
            !obj.contains_key("markdown_files"),
            "legacy alias markdown_files must not appear in crawl result JSON"
        );
    }

    #[test]
    fn crawl_result_json_canonical_key_set_is_exact() {
        let json = build_crawl_result_json("https://example.com", &make_summary(), None);
        let mut keys: Vec<&str> = json
            .as_object()
            .expect("json is an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();
        let expected = vec![
            "elapsed_ms",
            "embed_job_id",
            "error_pages",
            "md_created",
            "pages_crawled",
            "pages_discovered",
            "thin_md",
            "url",
            "waf_blocked_pages",
        ];
        assert_eq!(
            keys, expected,
            "canonical crawl result key set drift detected"
        );
    }
}
