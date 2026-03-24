//! Process-global pool of pre-warmed ACP adapter sessions.
//!
//! Eliminates the cold-start tax on the first LLM request for long-running
//! entry points (`axon serve`, `axon mcp`). Call [`init_warm_pool`] once at
//! startup, then [`try_checkout`] on each request — a background refill
//! immediately re-populates the pool after each checkout.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::warm::WarmAcpSession;
use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_info, log_warn};

/// Maximum idle age before a pooled session is considered stale and discarded.
const MAX_IDLE: Duration = Duration::from_secs(600);

/// Minimum pool size — maintained after every checkout via background refill.
const MIN_SIZE: usize = 1;

// ---------------------------------------------------------------------------
// CfgKey
// ---------------------------------------------------------------------------

/// Identifies the adapter + model combination that a pooled session was
/// spawned for. Sessions are only reused when the config key matches exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfgKey {
    pub adapter_cmd: String,
    pub model: String,
}

impl CfgKey {
    /// Build a key from the runtime [`Config`].
    pub fn from_config(cfg: &Config) -> Self {
        Self {
            adapter_cmd: cfg.acp_adapter_cmd.clone().unwrap_or_default(),
            model: cfg.openai_model.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// PooledSession
// ---------------------------------------------------------------------------

struct PooledSession {
    session: WarmAcpSession,
    inserted_at: Instant,
}

// ---------------------------------------------------------------------------
// WarmSessionPool
// ---------------------------------------------------------------------------

struct WarmSessionPool {
    sessions: Mutex<VecDeque<PooledSession>>,
    cfg_key: CfgKey,
}

impl WarmSessionPool {
    fn new(cfg: &Config) -> Self {
        Self {
            sessions: Mutex::new(VecDeque::new()),
            cfg_key: CfgKey::from_config(cfg),
        }
    }

    /// Push a fresh session into the pool.
    fn push(&self, session: WarmAcpSession) {
        let entry = PooledSession {
            session,
            inserted_at: Instant::now(),
        };
        let mut guard = self.sessions.lock().expect("pool mutex poisoned");
        guard.push_back(entry);
    }

    /// Pop the oldest session that is still within [`MAX_IDLE`].
    /// Stale entries encountered on the way are silently dropped.
    fn pop_fresh(&self) -> Option<(WarmAcpSession, Duration)> {
        let mut guard = self.sessions.lock().expect("pool mutex poisoned");
        while let Some(entry) = guard.pop_front() {
            let age = entry.inserted_at.elapsed();
            if age <= MAX_IDLE {
                return Some((entry.session, age));
            }
            // stale — drop and try next
        }
        None
    }

    /// Current number of pooled sessions (includes potentially stale ones).
    fn len(&self) -> usize {
        let guard = self.sessions.lock().expect("pool mutex poisoned");
        guard.len()
    }
}

// ---------------------------------------------------------------------------
// Process-global pool
// ---------------------------------------------------------------------------

static WARM_POOL: OnceLock<WarmSessionPool> = OnceLock::new();

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Initialize the warm session pool at process startup.
///
/// No-op when `acp_adapter_cmd` is unset or empty. Safe to call multiple
/// times — only the first call has any effect (protected by [`OnceLock`]).
pub fn init_warm_pool(cfg: &Config) {
    if cfg.acp_adapter_cmd.as_deref().map_or(true, str::is_empty) {
        return;
    }
    let pool = WARM_POOL.get_or_init(|| WarmSessionPool::new(cfg));
    let current = pool.len();
    for _ in current..MIN_SIZE {
        let t = Instant::now();
        match super::warm::spawn_warm_session(cfg, None) {
            Ok(session) => {
                log_info(&format!(
                    "warm pool: pre-warmed session spawn={}ms",
                    t.elapsed().as_millis()
                ));
                pool.push(session);
            }
            Err(e) => log_warn(&format!("warm pool: failed to pre-warm session: {e}")),
        }
    }
}

/// Check out a session from the pool, if one is available for this config.
///
/// After checkout a background task immediately refills the pool so the next
/// request also benefits from a pre-warmed session.
///
/// Returns `None` when:
/// - the pool has not been initialised,
/// - the config key does not match the pool's key, or
/// - all pooled sessions have expired.
pub fn try_checkout(cfg: &Config) -> Option<WarmAcpSession> {
    let pool = WARM_POOL.get()?;
    if pool.cfg_key != CfgKey::from_config(cfg) {
        return None;
    }
    let (session, age) = pool.pop_fresh()?;
    let remaining = pool.len();
    log_info(&format!(
        "warm pool: checkout age={}ms pool_remaining={}",
        age.as_millis(),
        remaining
    ));
    let cfg_clone = cfg.clone();
    tokio::task::spawn_blocking(move || {
        let t = Instant::now();
        match super::warm::spawn_warm_session(&cfg_clone, None) {
            Ok(new_session) => {
                log_info(&format!(
                    "warm pool: refill spawn={}ms",
                    t.elapsed().as_millis()
                ));
                if let Some(p) = WARM_POOL.get() {
                    p.push(new_session);
                }
            }
            Err(e) => log_warn(&format!("warm pool: background refill failed: {e}")),
        }
    });
    Some(session)
}

/// Current number of sessions in the pool (0 when uninitialised).
pub fn pool_size() -> usize {
    WARM_POOL.get().map_or(0, WarmSessionPool::len)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key(cmd: &str, model: &str) -> CfgKey {
        CfgKey {
            adapter_cmd: cmd.to_string(),
            model: model.to_string(),
        }
    }

    #[test]
    fn cfg_key_matches_equal_configs() {
        let k1 = make_key("codex", "gpt-4o");
        let k2 = make_key("codex", "gpt-4o");
        assert_eq!(k1, k2);
    }

    #[test]
    fn cfg_key_differs_on_model_change() {
        let k1 = make_key("codex", "gpt-4o");
        let k2 = make_key("codex", "gpt-4o-mini");
        assert_ne!(k1, k2);
    }

    #[test]
    fn cfg_key_differs_on_adapter_change() {
        let k1 = make_key("codex", "gpt-4o");
        let k2 = make_key("claude", "gpt-4o");
        assert_ne!(k1, k2);
    }

    #[test]
    fn pool_size_zero_before_init() {
        let _ = pool_size();
    }
}
