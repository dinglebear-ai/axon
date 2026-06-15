use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::crawl::engine::CrawlSummary;
use crate::jobs::backend::JobKind;
use crate::jobs::ops::update_result_json_for_attempt;
use crate::vector::ops::tei::EmbedProgress;

pub(super) fn spawn_crawl_progress_persister(
    pool: &SqlitePool,
    id: uuid::Uuid,
    attempt_id: Option<String>,
    output_dir: std::path::PathBuf,
) -> (mpsc::Sender<CrawlSummary>, tokio::task::JoinHandle<()>) {
    let pool = pool.clone();
    let (tx, mut rx) = mpsc::channel::<CrawlSummary>(32);
    let task = tokio::spawn(async move {
        while let Some(summary) = rx.recv().await {
            let mut progress = serde_json::json!({
                "output_dir": output_dir,
                "output_path": output_dir.join("markdown"),
                "pages_crawled": summary.pages_seen,
                "pages_discovered": summary.pages_discovered,
                "queued": summary.queued(),
                "depth_max": summary.depth_max,
                "md_created": summary.markdown_files,
                "thin_md": summary.thin_pages,
                "error_pages": summary.error_pages,
                "waf_blocked_pages": summary.waf_blocked_pages,
                "reused_pages": summary.reused_pages,
                "diagnostic_count": summary.diagnostics.len(),
                "events": summary.recent_events,
                "rate_limited": summary.rate_limited,
            });
            if let (Some(adaptive), Some(obj)) =
                (summary.adaptive.as_ref(), progress.as_object_mut())
            {
                obj.insert(
                    "adaptive_concurrency".to_string(),
                    serde_json::to_value(adaptive).unwrap_or(serde_json::Value::Null),
                );
            }
            if let Err(e) = update_result_json_for_attempt(
                &pool,
                JobKind::Crawl,
                id,
                attempt_id.as_deref(),
                &progress,
            )
            .await
            {
                tracing::warn!(job_id = %id, error = %e, "failed to persist crawl progress");
            }
        }
    });
    (tx, task)
}

pub(super) fn spawn_embed_progress_persister(
    pool: &SqlitePool,
    id: uuid::Uuid,
    attempt_id: Option<String>,
) -> (mpsc::Sender<EmbedProgress>, tokio::task::JoinHandle<()>) {
    let pool = pool.clone();
    let (tx, mut rx) = mpsc::channel::<EmbedProgress>(32);
    let task = tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let json = serde_json::json!({
                "docs_total": progress.docs_total,
                "docs_embedded": progress.docs_completed,
                "chunks_embedded": progress.chunks_embedded,
            });
            if let Err(e) = update_result_json_for_attempt(
                &pool,
                JobKind::Embed,
                id,
                attempt_id.as_deref(),
                &json,
            )
            .await
            {
                tracing::warn!(job_id = %id, error = %e, "failed to persist embed progress");
            }
        }
    });
    (tx, task)
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
