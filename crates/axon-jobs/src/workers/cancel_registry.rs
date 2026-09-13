use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use axon_api::source::JobId;
use tokio_util::sync::CancellationToken;

type AttemptKey = (JobId, u32);

static TOKENS: OnceLock<Mutex<HashMap<AttemptKey, CancellationToken>>> = OnceLock::new();

fn tokens() -> &'static Mutex<HashMap<AttemptKey, CancellationToken>> {
    TOKENS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_tokens() -> MutexGuard<'static, HashMap<AttemptKey, CancellationToken>> {
    match tokens().lock() {
        Ok(tokens) => tokens,
        // The registry contains no compound invariant that becomes unsafe
        // after an unrelated worker panics, so retain the live token map.
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub(crate) fn register(
    job_id: JobId,
    attempt: u32,
    shutdown: &CancellationToken,
) -> CancellationToken {
    let token = shutdown.child_token();
    lock_tokens().insert((job_id, attempt), token.clone());
    token
}

pub(crate) fn unregister(job_id: JobId, attempt: u32) {
    lock_tokens().remove(&(job_id, attempt));
}

pub(crate) fn cancel_attempt(job_id: JobId, attempt: u32) -> bool {
    let token = lock_tokens().get(&(job_id, attempt)).cloned();
    if let Some(token) = token {
        token.cancel();
        true
    } else {
        false
    }
}

pub(crate) fn cancel_job(job_id: JobId) -> bool {
    let tokens = lock_tokens()
        .iter()
        .filter(|((registered_job_id, _), _)| *registered_job_id == job_id)
        .map(|(_, token)| token.clone())
        .collect::<Vec<_>>();
    for token in &tokens {
        token.cancel();
    }
    !tokens.is_empty()
}

#[cfg(test)]
#[path = "cancel_registry_tests.rs"]
mod tests;
