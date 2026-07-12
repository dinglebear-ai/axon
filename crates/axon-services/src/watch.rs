use std::error::Error;

use sqlx::SqlitePool;
use uuid::Uuid;

use axon_core::config::Config;
use axon_jobs::watch;

pub use axon_jobs::watch::{
    WatchDef, WatchDefCreate, WatchDefCreateRequest, WatchRun, WatchRunArtifact,
};

// Source-request-backed watch store (WS-B / audit C4-04, issue #298). This is
// a thin facade over `axon_jobs::watch_store::SqliteWatchStore` — the real
// `WatchStore` implementation — kept deliberately separate from the
// task_type/task_payload facade above (see `watch_store.rs` module docs for
// why the two models are not unified in this slice).
pub use axon_api::source::{
    AdapterOptions, WatchId, WatchListRequest, WatchRequest, WatchResult, WatchSchedule,
    WatchSummary, WatchUpdateRequest,
};
pub use axon_jobs::boundary::WatchStore as SourceWatchStoreTrait;
pub use axon_jobs::watch_store::SqliteWatchStore;

/// Open a [`SqliteWatchStore`] against the given pool, or against a freshly
/// opened config-derived pool when no shared pool is available (mirrors the
/// `shared_pool`/`cfg` fallback pattern used by the legacy watch facade
/// above).
pub async fn open_source_watch_store(
    cfg: &Config,
    pool: Option<&SqlitePool>,
) -> Result<SqliteWatchStore, Box<dyn Error>> {
    let pool = match pool {
        Some(pool) => pool.clone(),
        None => axon_jobs::store::open_config_pool(cfg).await?,
    };
    Ok(SqliteWatchStore::new(pool))
}

/// Create a source-request-backed watch (issue #298 WS-B `POST /v1/watches`).
///
/// Writes the canonical row via [`SqliteWatchStore`] — the store that
/// `list`/`get`/`update`/`pause`/`resume`/`delete` act on — then best-effort
/// dual-writes a legacy `axon_watch_defs` row so the still-live scheduler
/// (`crates/axon-jobs/src/workers/watch_scheduler.rs`) actually ticks the
/// watch. Mirrors the dual-write `axon watch create` performs in the opposite
/// direction (`crates/axon-cli/src/commands/watch.rs::handle_watch_create`).
/// The dual-write is additive: a failure there is logged and does not fail
/// the request — the canonical watch was already persisted and remains fully
/// functional through `get`/`update`/`pause`/`resume`/`delete`.
pub async fn create_source_watch(
    cfg: &Config,
    pool: Option<&SqlitePool>,
    request: WatchRequest,
) -> Result<WatchResult, Box<dyn Error>> {
    let store = open_source_watch_store(cfg, pool).await?;
    let created = SourceWatchStoreTrait::create(&store, request.clone()).await?;

    let legacy_input = WatchDefCreateRequest {
        name: format!("watch-{}", created.watch_id.0),
        task_type: "watch".to_string(),
        task_payload: serde_json::json!({ "urls": [request.source] }),
        every_seconds: request.schedule.every_seconds as i64,
        enabled: request.enabled,
        next_run_at: None,
    }
    .into_create();
    match legacy_input {
        Ok(input) => {
            let legacy_result = match pool {
                Some(pool) => create_watch_def_with_pool(pool, &input).await,
                None => create_watch_def(cfg, &input).await,
            };
            if let Err(err) = legacy_result {
                axon_core::logging::log_warn(&format!(
                    "watch create: dual-write to legacy watch scheduler failed: {err} \
                     (source watch {} was still created; the recurring scheduler will not tick it)",
                    created.watch_id.0
                ));
            }
        }
        Err(msg) => {
            axon_core::logging::log_warn(&format!(
                "watch create: skipped legacy scheduler dual-write for source watch {} \
                 ({msg}); the recurring scheduler will not tick it",
                created.watch_id.0
            ));
        }
    }
    Ok(created)
}

pub async fn list_watch_defs(cfg: &Config, limit: i64) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    watch::list_watch_defs(cfg, limit).await
}

pub async fn list_watch_defs_with_pool(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    watch::list_watch_defs_with_pool(pool, limit).await
}

pub async fn create_watch_def(
    cfg: &Config,
    input: &WatchDefCreate,
) -> Result<WatchDef, Box<dyn Error>> {
    watch::create_watch_def(cfg, input).await
}

pub async fn create_watch_def_with_pool(
    pool: &SqlitePool,
    input: &WatchDefCreate,
) -> Result<WatchDef, Box<dyn Error>> {
    watch::create_watch_def_with_pool(pool, input).await
}

pub async fn list_watch_runs(
    cfg: &Config,
    watch_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRun>, Box<dyn Error>> {
    watch::list_watch_runs(cfg, watch_id, limit).await
}

pub async fn list_watch_runs_with_pool(
    pool: &SqlitePool,
    watch_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRun>, Box<dyn Error>> {
    watch::list_watch_runs_with_pool(pool, watch_id, limit).await
}

pub async fn list_watch_run_artifacts(
    cfg: &Config,
    run_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRunArtifact>, Box<dyn Error>> {
    watch::list_watch_run_artifacts(cfg, run_id, limit).await
}

pub async fn list_watch_run_artifacts_with_pool(
    pool: &SqlitePool,
    run_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRunArtifact>, Box<dyn Error>> {
    watch::list_watch_run_artifacts_with_pool(pool, run_id, limit).await
}

pub async fn create_watch_run(
    cfg: &Config,
    watch_id: Uuid,
    dispatched_job_id: Option<Uuid>,
) -> Result<WatchRun, Box<dyn Error>> {
    watch::create_watch_run(cfg, watch_id, dispatched_job_id).await
}

pub async fn get_watch_def(
    cfg: &Config,
    watch_id: Uuid,
) -> Result<Option<WatchDef>, Box<dyn Error>> {
    watch::get_watch_def(cfg, watch_id).await
}

pub async fn get_watch_def_with_pool(
    pool: &SqlitePool,
    watch_id: Uuid,
) -> Result<Option<WatchDef>, Box<dyn Error>> {
    watch::get_watch_def_with_pool(pool, watch_id).await
}

/// Look up a legacy `WatchDef` by its unique dual-write `name` (see
/// `crate::watch::create_source_watch` and `axon_jobs::watch::
/// get_watch_def_by_name`).
pub async fn get_watch_def_by_name(
    cfg: &Config,
    name: &str,
) -> Result<Option<WatchDef>, Box<dyn Error>> {
    watch::get_watch_def_by_name(cfg, name).await
}

pub async fn get_watch_def_by_name_with_pool(
    pool: &SqlitePool,
    name: &str,
) -> Result<Option<WatchDef>, Box<dyn Error>> {
    watch::get_watch_def_by_name_with_pool(pool, name).await
}

pub async fn finish_watch_run(
    cfg: &Config,
    watch_id: Uuid,
    run_id: Uuid,
    status: &str,
    result_json: Option<&serde_json::Value>,
    error_text: Option<&str>,
) -> Result<bool, Box<dyn Error>> {
    watch::finish_watch_run(cfg, watch_id, run_id, status, result_json, error_text).await
}

pub async fn run_watch_now(cfg: &Config, watch: &WatchDef) -> Result<WatchRun, Box<dyn Error>> {
    watch::run_watch_now(cfg, watch).await
}

pub async fn run_watch_now_with_pool(
    cfg: &Config,
    pool: &SqlitePool,
    watch: &WatchDef,
) -> Result<WatchRun, Box<dyn Error>> {
    watch::run_watch_now_with_pool(cfg, pool, watch).await
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
