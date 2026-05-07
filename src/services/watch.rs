use std::error::Error;

use uuid::Uuid;

use crate::core::config::Config;
use crate::jobs::watch_lite::{self as watch_lite};

pub use crate::jobs::watch_lite::{WatchDef, WatchDefCreate, WatchRun};

pub async fn list_watch_defs(cfg: &Config, limit: i64) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    watch_lite::list_watch_defs(cfg, limit).await
}

pub async fn create_watch_def(
    cfg: &Config,
    input: &WatchDefCreate,
) -> Result<WatchDef, Box<dyn Error>> {
    watch_lite::create_watch_def(cfg, input).await
}

pub async fn list_watch_runs(
    cfg: &Config,
    watch_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRun>, Box<dyn Error>> {
    watch_lite::list_watch_runs(cfg, watch_id, limit).await
}

pub async fn create_watch_run(
    cfg: &Config,
    watch_id: Uuid,
    dispatched_job_id: Option<Uuid>,
) -> Result<WatchRun, Box<dyn Error>> {
    watch_lite::create_watch_run(cfg, watch_id, dispatched_job_id).await
}

pub async fn get_watch_def(
    cfg: &Config,
    watch_id: Uuid,
) -> Result<Option<WatchDef>, Box<dyn Error>> {
    watch_lite::get_watch_def(cfg, watch_id).await
}

pub async fn finish_watch_run(
    cfg: &Config,
    watch_id: Uuid,
    run_id: Uuid,
    status: &str,
    result_json: Option<&serde_json::Value>,
    error_text: Option<&str>,
) -> Result<bool, Box<dyn Error>> {
    watch_lite::finish_watch_run(cfg, watch_id, run_id, status, result_json, error_text).await
}

pub async fn run_watch_now(cfg: &Config, watch: &WatchDef) -> Result<WatchRun, Box<dyn Error>> {
    watch_lite::run_watch_now(cfg, watch).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::Config;
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
