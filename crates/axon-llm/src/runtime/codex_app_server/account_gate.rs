//! Exclusive canonical-auth ownership between synthesis and account sessions.
use std::sync::{Arc, LazyLock};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

static GATE: LazyLock<Arc<RwLock<()>>> = LazyLock::new(|| Arc::new(RwLock::new(())));

pub(super) fn completion_guard() -> Result<OwnedRwLockReadGuard<()>, String> {
    GATE.clone().try_read_owned().map_err(|_| {
        "synthesis_account_busy: device sign-in owns Codex authentication; finish or cancel sign-in before generating".into()
    })
}

/// Fails before changing auth if an active completion owns a pool child.
pub async fn acquire() -> Result<OwnedRwLockWriteGuard<()>, String> {
    let guard = GATE.clone().try_write_owned().map_err(|_| {
        "synthesis_account_busy: generation is active; wait for it to finish before signing in"
            .to_string()
    })?;
    if let Err(error) = super::pool::drain_for_account_change().await {
        // Unknown child liveness must not permit a concurrent canonical auth writer.
        std::mem::forget(guard);
        return Err(error);
    }
    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn login_excludes_completions_and_cannot_interrupt_active_generation() {
        let active = completion_guard().unwrap();
        assert!(
            acquire()
                .await
                .unwrap_err()
                .contains("generation is active")
        );
        drop(active);
        let login = acquire().await.unwrap();
        assert!(
            completion_guard()
                .unwrap_err()
                .contains("synthesis_account_busy")
        );
        assert!(acquire().await.is_err());
        drop(login);
        assert!(completion_guard().is_ok());
    }
}
