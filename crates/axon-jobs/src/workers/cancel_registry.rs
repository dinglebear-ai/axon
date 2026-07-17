use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use axon_api::source::JobId;
use tokio_util::sync::CancellationToken;

static TOKENS: OnceLock<Mutex<HashMap<JobId, (u32, CancellationToken)>>> = OnceLock::new();

fn tokens() -> &'static Mutex<HashMap<JobId, (u32, CancellationToken)>> {
    TOKENS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_tokens() -> MutexGuard<'static, HashMap<JobId, (u32, CancellationToken)>> {
    match tokens().lock() {
        Ok(tokens) => tokens,
        // The registry contains no compound invariant that becomes unsafe
        // after an unrelated worker panics, so retain the live token map.
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub(super) fn register(
    job_id: JobId,
    attempt: u32,
    shutdown: &CancellationToken,
) -> CancellationToken {
    let token = shutdown.child_token();
    lock_tokens().insert(job_id, (attempt, token.clone()));
    token
}

pub(super) fn unregister(job_id: JobId, attempt: u32) {
    let mut tokens = lock_tokens();
    if tokens
        .get(&job_id)
        .is_some_and(|(registered_attempt, _)| *registered_attempt == attempt)
    {
        tokens.remove(&job_id);
    }
}

pub(crate) fn cancel_job(job_id: JobId) -> bool {
    let token = lock_tokens().get(&job_id).map(|(_, token)| token.clone());
    if let Some(token) = token {
        token.cancel();
        true
    } else {
        false
    }
}
