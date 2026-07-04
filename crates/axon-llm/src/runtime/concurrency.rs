use std::error::Error as StdError;
use std::sync::{Arc, LazyLock};

use dashmap::DashMap;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Typed key for the per-backend LLM completion semaphore map (B-M3).
///
/// Eliminates per-call `String` allocations from `completion_limiter_key` and
/// makes the key type explicit.  `Default` covers the legacy
/// `acquire_completion_permit` call-site.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum CompletionKey {
    Gemini { cmd: String, model: String },
    OpenAi { base_url: String, model: String },
    Codex { cmd: String, model: String },
    Default,
}

#[cfg(test)]
const DEFAULT_LLM_COMPLETION_CONCURRENCY: usize = 4;

static COMPLETION_SEMAPHORES: LazyLock<DashMap<CompletionKey, Arc<Semaphore>>> =
    LazyLock::new(DashMap::new);

#[cfg(test)]
fn parse_completion_concurrency_limit(raw: Option<&str>) -> usize {
    raw.and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(Semaphore::MAX_PERMITS))
        .unwrap_or(DEFAULT_LLM_COMPLETION_CONCURRENCY)
}

pub async fn acquire_completion_permit(
    limit: usize,
) -> Result<OwnedSemaphorePermit, Box<dyn StdError + Send + Sync>> {
    acquire_completion_permit_for_key(CompletionKey::Default, limit).await
}

pub(crate) async fn acquire_completion_permit_for_key(
    key: CompletionKey,
    limit: usize,
) -> Result<OwnedSemaphorePermit, Box<dyn StdError + Send + Sync>> {
    completion_semaphore_for_key(key, limit)
        .acquire_owned()
        .await
        .map_err(|err| format!("LLM completion semaphore closed: {err}").into())
}

fn completion_semaphore_for_key(key: CompletionKey, limit: usize) -> Arc<Semaphore> {
    let normalized_limit = limit.clamp(1, Semaphore::MAX_PERMITS);
    COMPLETION_SEMAPHORES
        .entry(key)
        .or_insert_with(|| Arc::new(Semaphore::new(normalized_limit)))
        .clone()
}

#[cfg(test)]
fn reset_completion_limiters_for_tests() {
    COMPLETION_SEMAPHORES.clear();
}

#[cfg(test)]
fn available_permits_for_key(key: &CompletionKey) -> Option<usize> {
    COMPLETION_SEMAPHORES
        .get(key)
        .map(|semaphore| semaphore.available_permits())
}

#[cfg(test)]
fn completion_semaphore_for_key_for_tests(key: CompletionKey, limit: usize) -> Arc<Semaphore> {
    completion_semaphore_for_key(key, limit)
}

#[cfg(test)]
#[path = "concurrency_tests.rs"]
mod tests;
