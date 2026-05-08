use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::crawl::engine::CrawlSummary;
use crate::jobs::backend::JobKind;
use crate::jobs::lite::ops::update_result_json;
use crate::vector::ops::tei::EmbedProgress;

pub(super) fn spawn_crawl_progress_persister(
    pool: &SqlitePool,
    id: uuid::Uuid,
    output_dir: std::path::PathBuf,
) -> (mpsc::Sender<CrawlSummary>, tokio::task::JoinHandle<()>) {
    let pool = pool.clone();
    let (tx, mut rx) = mpsc::channel::<CrawlSummary>(32);
    let task = tokio::spawn(async move {
        while let Some(summary) = rx.recv().await {
            let progress = serde_json::json!({
                "output_dir": output_dir,
                "output_path": output_dir.join("markdown"),
                "pages_crawled": summary.pages_seen,
                "md_created": summary.markdown_files,
                "thin_md": summary.thin_pages,
            });
            if let Err(e) = update_result_json(&pool, JobKind::Crawl, id, &progress).await {
                tracing::warn!(job_id = %id, error = %e, "failed to persist crawl progress");
            }
        }
    });
    (tx, task)
}

pub(super) fn spawn_embed_progress_persister(
    pool: &SqlitePool,
    id: uuid::Uuid,
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
            if let Err(e) = update_result_json(&pool, JobKind::Embed, id, &json).await {
                tracing::warn!(job_id = %id, error = %e, "failed to persist embed progress");
            }
        }
    });
    (tx, task)
}
