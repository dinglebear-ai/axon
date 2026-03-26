use std::error::Error;

use uuid::Uuid;

use crate::crates::core::config::Config;
use crate::crates::jobs::watch::{self as watch_jobs, WatchDef, WatchRun};
use crate::crates::jobs::watch_lite;

pub use crate::crates::jobs::watch::WatchDefCreate;

pub async fn list_watch_defs(cfg: &Config, limit: i64) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    if cfg.lite_mode {
        return watch_lite::list_watch_defs(cfg, limit).await;
    }
    watch_jobs::list_watch_defs(cfg, limit).await
}

pub async fn create_watch_def(
    cfg: &Config,
    input: &WatchDefCreate,
) -> Result<WatchDef, Box<dyn Error>> {
    if cfg.lite_mode {
        return watch_lite::create_watch_def(cfg, input).await;
    }
    watch_jobs::create_watch_def(cfg, input).await
}

pub async fn list_watch_runs(
    cfg: &Config,
    watch_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRun>, Box<dyn Error>> {
    if cfg.lite_mode {
        return watch_lite::list_watch_runs(cfg, watch_id, limit).await;
    }
    watch_jobs::list_watch_runs(cfg, watch_id, limit).await
}

pub async fn create_watch_run(
    cfg: &Config,
    watch_id: Uuid,
    dispatched_job_id: Option<Uuid>,
) -> Result<WatchRun, Box<dyn Error>> {
    if cfg.lite_mode {
        return watch_lite::create_watch_run(cfg, watch_id, dispatched_job_id).await;
    }
    watch_jobs::create_watch_run(cfg, watch_id, dispatched_job_id).await
}

pub async fn get_watch_def(
    cfg: &Config,
    watch_id: Uuid,
) -> Result<Option<WatchDef>, Box<dyn Error>> {
    if cfg.lite_mode {
        return watch_lite::get_watch_def(cfg, watch_id).await;
    }
    Ok(list_watch_defs(cfg, 500)
        .await?
        .into_iter()
        .find(|watch| watch.id == watch_id))
}

pub async fn finish_watch_run(
    cfg: &Config,
    watch_id: Uuid,
    run_id: Uuid,
    status: &str,
    result_json: Option<&serde_json::Value>,
    error_text: Option<&str>,
) -> Result<bool, Box<dyn Error>> {
    if cfg.lite_mode {
        return watch_lite::finish_watch_run(
            cfg,
            watch_id,
            run_id,
            status,
            result_json,
            error_text,
        )
        .await;
    }
    watch_jobs::mark_watch_run_finished_with_pool(
        &crate::crates::jobs::common::make_pool(cfg).await?,
        watch_id,
        run_id,
        status,
        result_json,
        error_text,
    )
    .await
}

pub async fn run_watch_now(cfg: &Config, watch: &WatchDef) -> Result<WatchRun, Box<dyn Error>> {
    if cfg.lite_mode {
        return watch_lite::run_watch_now(cfg, watch).await;
    }

    let dispatched_job_id = if watch.task_type == "refresh" {
        let urls = watch
            .task_payload
            .get("urls")
            .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
            .unwrap_or_default();
        if urls.is_empty() {
            None
        } else {
            Some(Uuid::parse_str(
                &crate::crates::services::refresh::refresh_start(cfg, &urls)
                    .await?
                    .job_id,
            )?)
        }
    } else {
        None
    };
    create_watch_run(cfg, watch.id, dispatched_job_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::Config;
    use crate::crates::jobs::watch::{WatchDef, WatchDefCreate, WatchRun};
    use uuid::Uuid;

    #[allow(dead_code)]
    fn _assert_signatures() {
        async fn _f1(cfg: &Config) {
            let _: Result<Vec<WatchDef>, _> = list_watch_defs(cfg, 10_i64).await;
        }
        async fn _f2(cfg: &Config, input: &WatchDefCreate) {
            let _: Result<WatchDef, _> = create_watch_def(cfg, input).await;
        }
        async fn _f3(cfg: &Config, id: Uuid) {
            let _: Result<Vec<WatchRun>, _> = list_watch_runs(cfg, id, 10_i64).await;
        }
    }
}
