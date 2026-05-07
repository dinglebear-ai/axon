use std::error::Error as StdError;
use std::sync::{Arc, OnceLock};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[cfg(test)]
const DEFAULT_LLM_COMPLETION_CONCURRENCY: usize = 4;

static COMPLETION_SEMAPHORE: OnceLock<Arc<Semaphore>> = OnceLock::new();

#[cfg(test)]
fn parse_completion_concurrency_limit(raw: Option<&str>) -> usize {
    raw.and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_LLM_COMPLETION_CONCURRENCY)
}

pub async fn acquire_completion_permit(
    limit: usize,
) -> Result<OwnedSemaphorePermit, Box<dyn StdError + Send + Sync>> {
    COMPLETION_SEMAPHORE
        .get_or_init(|| Arc::new(Semaphore::new(limit.max(1))))
        .clone()
        .acquire_owned()
        .await
        .map_err(|err| format!("LLM completion semaphore closed: {err}").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_concurrency_defaults_to_four() {
        assert_eq!(parse_completion_concurrency_limit(None), 4);
    }

    #[test]
    fn completion_concurrency_rejects_zero() {
        assert_eq!(parse_completion_concurrency_limit(Some("0")), 4);
    }
}
