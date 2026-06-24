use super::*;
use axon_core::config::Config;
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
