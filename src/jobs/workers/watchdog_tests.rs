use super::*;

use crate::jobs::cancel::CancelStore;
use crate::jobs::store::{ReclaimedJob, ReclaimedJobs};

#[test]
fn watchdog_reclaim_cancels_local_tokens_before_retry_notify() {
    let cancel_store = CancelStore::new();
    let id = uuid::Uuid::new_v4();
    let token = cancel_store.register(id, "attempt-1");
    let reclaimed = ReclaimedJobs {
        embed: vec![ReclaimedJob {
            id,
            attempt_id: Some("attempt-1".to_string()),
        }],
        ..Default::default()
    };

    cancel_reclaimed_local_tokens(&cancel_store, &reclaimed);

    assert!(token.is_cancelled(), "old local owner must be canceled");
    assert!(
        !cancel_store.cancel_local(id, "attempt-1"),
        "token should be removed after watchdog local cancel"
    );
}

#[test]
fn watchdog_reclaim_does_not_cancel_new_attempt_token() {
    let cancel_store = CancelStore::new();
    let id = uuid::Uuid::new_v4();
    let old_token = cancel_store.register(id, "attempt-1");
    let new_token = cancel_store.register(id, "attempt-2");
    let reclaimed = ReclaimedJobs {
        embed: vec![ReclaimedJob {
            id,
            attempt_id: Some("attempt-1".to_string()),
        }],
        ..Default::default()
    };

    cancel_reclaimed_local_tokens(&cancel_store, &reclaimed);

    assert!(old_token.is_cancelled(), "stale attempt should be canceled");
    assert!(
        !new_token.is_cancelled(),
        "fresh retry attempt must not be canceled by stale reclaim"
    );
}
