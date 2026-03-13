use std::sync::Arc;

use crate::crates::core::config::Config;

/// Typed error alias for service call wrappers — erased to `String` only at the WS boundary.
pub(crate) type SvcError = Box<dyn std::error::Error + Send + Sync + 'static>;

// ACP session concurrency is enforced by crate::crates::web::ACP_SESSION_SEMAPHORE
// (acquired in execute.rs before calling handle_sync_direct).  Do NOT add a second
// semaphore here — dual acquisition cuts effective capacity and creates two
// inconsistent sources of truth for the AXON_ACP_MAX_CONCURRENT_SESSIONS limit.

/// Modes dispatched directly through service functions (no subprocess).
///
/// This constant is the authoritative list consumed by tests and must stay in
/// sync with [`ServiceMode`].  At runtime `classify_sync_direct` uses
/// `ServiceMode::from_str` directly, so this constant is dead in non-test
/// builds — the `allow` below silences the warning intentionally.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const DIRECT_SYNC_MODES: &[&str] = &[
    "scrape",
    "map",
    "query",
    "retrieve",
    "ask",
    "search",
    "research",
    "stats",
    "sources",
    "domains",
    "doctor",
    "status",
    "suggest",
    "evaluate",
    "dedupe",
    "screenshot",
    "debug",
    "sessions",
    "mcp_refresh",
    "pulse_chat",
    "pulse_chat_probe",
];

/// Owned parameters extracted from the WS request before any `.await`.
///
/// All fields are owned so the containing future is `Send + 'static`.
/// Visibility is `pub(super)` so `execute.rs` can pass the opaque value from
/// `classify_sync_direct` to `handle_sync_direct` without inspecting its fields.
///
/// `cfg` is kept as `Arc<Config>` (not a plain `Config`) so that the
/// `call_*` service wrappers can clone the `Arc` into `async move` blocks
/// and borrow from the Arc-owned data without exposing a lifetime parameter
/// to `tokio::task::spawn`'s `Send + 'static` check.
pub(crate) struct DirectParams {
    pub(super) mode: ServiceMode,
    pub(super) input: String,
    pub(super) cfg: Arc<Config>,
    pub(super) limit: usize,
    pub(super) offset: usize,
    pub(super) max_points: Option<usize>,
    pub(super) agent: PulseChatAgent,
    pub(super) session_id: Option<String>,
    pub(super) model: Option<String>,
    pub(super) session_mode: Option<String>,
    pub(super) enabled_mcp_servers: Option<Vec<String>>,
    pub(super) blocked_mcp_tools: Vec<String>,
    pub(super) assistant_mode: bool,
    /// Whether to grant the ACP adapter filesystem access.
    /// Sent by `use-axon-acp.ts` as `enable_fs` (default `true`).
    pub(super) enable_fs: bool,
    /// Whether to grant the ACP adapter terminal access.
    /// Sent by `use-axon-acp.ts` as `enable_terminal` (default `true`).
    pub(super) enable_terminal: bool,
    /// How long (in seconds) ACP waits for permission approval before timing out.
    /// Sent by `use-axon-acp.ts` as `permission_timeout_secs` (optional).
    pub(super) permission_timeout_secs: Option<u64>,
    /// How long (in seconds) the ACP adapter process is allowed to run.
    /// Sent by `use-axon-acp.ts` as `adapter_timeout_secs` (optional).
    pub(super) adapter_timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PulseChatAgent {
    Claude,
    Codex,
    Gemini,
}

impl PulseChatAgent {
    pub(super) fn from_flag(value: Option<&str>) -> Self {
        match value {
            Some(raw) if raw.eq_ignore_ascii_case("codex") => Self::Codex,
            Some(raw) if raw.eq_ignore_ascii_case("gemini") => Self::Gemini,
            _ => Self::Claude,
        }
    }
}

/// Classified service mode — replaces `mode: String` in `DirectParams` so the
/// async state machine never holds a `&str` borrow across `.await` points.
///
/// The `match mode.as_str()` scrutinee in `dispatch_service` would otherwise
/// create an `&str` borrow that the Rust async state machine includes in every
/// generated `Future::poll` state variant, causing an HRTB `Send` diagnostic
/// when the future is submitted to `tokio::task::spawn`.
///
/// By classifying the mode synchronously (before the first `.await`) we drop
/// the `&str` borrow before any suspension point, satisfying the
/// `Send + 'static` bound.
#[derive(Debug, Clone, Copy)]
pub(super) enum ServiceMode {
    Scrape,
    Map,
    Query,
    Retrieve,
    Ask,
    Search,
    Research,
    Stats,
    Sources,
    Domains,
    Doctor,
    Status,
    Suggest,
    Evaluate,
    Dedupe,
    Screenshot,
    Debug,
    Sessions,
    McpRefresh,
    PulseChat,
    PulseChatProbe,
}

impl ServiceMode {
    /// Classify a mode string.  Returns `None` for unknown modes.
    pub(super) fn from_str(s: &str) -> Option<Self> {
        match s {
            "scrape" => Some(Self::Scrape),
            "map" => Some(Self::Map),
            "query" => Some(Self::Query),
            "retrieve" => Some(Self::Retrieve),
            "ask" => Some(Self::Ask),
            "search" => Some(Self::Search),
            "research" => Some(Self::Research),
            "stats" => Some(Self::Stats),
            "sources" => Some(Self::Sources),
            "domains" => Some(Self::Domains),
            "doctor" => Some(Self::Doctor),
            "status" => Some(Self::Status),
            "suggest" => Some(Self::Suggest),
            "evaluate" => Some(Self::Evaluate),
            "dedupe" => Some(Self::Dedupe),
            "screenshot" => Some(Self::Screenshot),
            "debug" => Some(Self::Debug),
            "sessions" => Some(Self::Sessions),
            "mcp_refresh" => Some(Self::McpRefresh),
            "pulse_chat" => Some(Self::PulseChat),
            "pulse_chat_probe" => Some(Self::PulseChatProbe),
            _ => None,
        }
    }
}

/// Extract a `usize` from a flags JSON value, falling back to `default`.
pub(super) fn flag_usize(flags: &serde_json::Value, key: &str, default: usize) -> usize {
    flags
        .get(key)
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(default)
}

/// Extract an optional `usize` from a flags JSON value.
pub(super) fn flag_opt_usize(flags: &serde_json::Value, key: &str) -> Option<usize> {
    flags.get(key).and_then(|v| v.as_u64()).map(|n| n as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_usize_returns_default_on_missing_key() {
        let flags = serde_json::json!({});
        assert_eq!(flag_usize(&flags, "limit", 10), 10);
    }

    #[test]
    fn flag_usize_returns_value_when_present() {
        let flags = serde_json::json!({"limit": 42});
        assert_eq!(flag_usize(&flags, "limit", 10), 42);
    }

    #[test]
    fn flag_opt_usize_returns_none_on_missing_key() {
        let flags = serde_json::json!({});
        assert_eq!(flag_opt_usize(&flags, "limit"), None);
    }

    #[test]
    fn flag_opt_usize_returns_some_when_present() {
        let flags = serde_json::json!({"limit": 7});
        assert_eq!(flag_opt_usize(&flags, "limit"), Some(7));
    }

    #[test]
    fn service_mode_from_str_roundtrip() {
        for mode in DIRECT_SYNC_MODES {
            assert!(
                ServiceMode::from_str(mode).is_some(),
                "ServiceMode::from_str(\"{mode}\") should return Some"
            );
        }
    }
}
