use std::error::Error;

use uuid::Uuid;

use crate::crates::core::config::Config;
use crate::crates::jobs::watch::{self as watch_jobs, WatchDef, WatchRun};

pub use crate::crates::jobs::watch::WatchDefCreate;

pub async fn list_watch_defs(cfg: &Config, limit: i64) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    watch_jobs::list_watch_defs(cfg, limit).await
}

pub async fn create_watch_def(
    cfg: &Config,
    input: &WatchDefCreate,
) -> Result<WatchDef, Box<dyn Error>> {
    watch_jobs::create_watch_def(cfg, input).await
}

pub async fn list_watch_runs(
    cfg: &Config,
    watch_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRun>, Box<dyn Error>> {
    watch_jobs::list_watch_runs(cfg, watch_id, limit).await
}

pub async fn create_watch_run(
    cfg: &Config,
    watch_id: Uuid,
    dispatched_job_id: Option<Uuid>,
) -> Result<WatchRun, Box<dyn Error>> {
    watch_jobs::create_watch_run(cfg, watch_id, dispatched_job_id).await
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
