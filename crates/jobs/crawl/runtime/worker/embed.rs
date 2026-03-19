//! Embed job submission after crawl completes.

use crate::crates::core::logging::log_info;
use crate::crates::jobs::embed::start_embed_job_with_pool;
use sqlx::PgPool;
use std::error::Error;
use uuid::Uuid;

use super::job_context::JobExecutionContext;

pub(super) async fn maybe_enqueue_embed_job(
    pool: &PgPool,
    ctx: &JobExecutionContext,
    crawl_job_id: Uuid,
) -> Result<(), Box<dyn Error>> {
    if !ctx.job_cfg.embed {
        return Ok(());
    }
    let markdown_dir = ctx.job_cfg.output_dir.join("markdown");
    let embed_job_id =
        start_embed_job_with_pool(pool, &ctx.job_cfg, &markdown_dir.to_string_lossy(), None)
            .await?;
    log_info(&format!(
        "command=crawl enqueue_embed crawl_job_id={} embed_job_id={}",
        crawl_job_id, embed_job_id
    ));
    Ok(())
}
