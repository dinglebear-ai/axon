use super::AgentTurnStore;
use std::{future::Future, time::Duration};

pub(super) async fn await_with_renewal<F: Future>(
    store: &AgentTurnStore,
    turn_id: &str,
    lease_version: u64,
    future: F,
) -> F::Output {
    tokio::pin!(future);
    let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
    heartbeat.tick().await;
    loop {
        tokio::select! {
            output = &mut future => return output,
            _ = heartbeat.tick() => {
                let _ = store.renew_lease(turn_id, lease_version, now_ms());
            },
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}
