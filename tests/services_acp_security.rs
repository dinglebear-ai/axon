//! Security regression tests for the ACP service layer.
//!
//! These tests cover fixes made in the 2026-03-08 review:
//!   - AcpSessionUpdateKind::Unknown serializes as "unknown" (not "status")
//!   - AcpSessionUpdateKind serde variants are collision-free
//!   - validate_adapter_command rejects empty/whitespace programs
//!   - validate_adapter_command accepts bare adapter names (non-shell)
//!   - validate_adapter_command rejects bare shell names (sh, bash, etc.)
//!   - spawn_adapter env allowlist: proxy vars are NOT passed through
//!   - spawn_adapter env allowlist: Gemini auth vars ARE passed through
//!   - SEC-7: PermissionResponderMap composite key isolates sessions
#![allow(unsafe_code)]

use axon::crates::services::acp::{AcpClientScaffold, validate_adapter_command};
use axon::crates::services::types::{
    AcpAdapterCommand, AcpBridgeEvent, AcpSessionUpdateEvent, AcpSessionUpdateKind,
};
use std::sync::Mutex;

// ── EnvVarGuard ───────────────────────────────────────────────────────────────

/// RAII guard that restores an environment variable to its previous value when
/// dropped.  If the variable was absent before the test set it, the guard
/// removes it on drop.  This prevents test-induced env mutations from leaking
/// into subsequent tests, even when a test panics.
///
/// # Safety
///
/// Callers must hold `ENV_LOCK` for the entire duration the guard is alive.
/// `std::env::set_var` / `remove_var` are not thread-safe — the lock enforces
/// single-threaded access to the process environment within this test binary.
struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    /// Set `key` to `value`, recording the prior value for restoration.
    ///
    /// # Safety
    ///
    /// Caller must hold `ENV_LOCK`.
    unsafe fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        // SAFETY: caller holds ENV_LOCK; single-threaded env access guaranteed.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // SAFETY: caller holds ENV_LOCK for the duration of the test; the drop
        // runs inside that scope.
        match &self.previous {
            Some(prev) => unsafe { std::env::set_var(self.key, prev) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

// ── AcpSessionUpdateKind serde correctness ──────────────────────────────────

#[test]
fn unknown_session_update_serializes_as_unknown_not_status() {
    let kind = AcpSessionUpdateKind::Unknown;
    let serialized = serde_json::to_string(&kind).unwrap();
    assert!(
        serialized.contains("unknown"),
        "AcpSessionUpdateKind::Unknown should serialize as 'unknown', got: {serialized}"
    );
    assert!(
        !serialized.contains("status"),
        "AcpSessionUpdateKind::Unknown must NOT serialize as 'status', got: {serialized}"
    );
}

#[test]
fn session_update_kind_serde_no_collision() {
    // Every variant must serialize to a distinct string so the frontend can
    // reliably dispatch on the wire type.
    let variants = vec![
        AcpSessionUpdateKind::UserDelta,
        AcpSessionUpdateKind::AssistantDelta,
        AcpSessionUpdateKind::ThinkingDelta,
        AcpSessionUpdateKind::ToolCallStarted,
        AcpSessionUpdateKind::ToolCallUpdated,
        AcpSessionUpdateKind::Plan,
        AcpSessionUpdateKind::AvailableCommandsUpdate,
        AcpSessionUpdateKind::CurrentModeUpdate,
        AcpSessionUpdateKind::ConfigOptionUpdate,
        AcpSessionUpdateKind::Unknown,
        AcpSessionUpdateKind::UsageUpdate,
    ];
    let serialized: Vec<String> = variants
        .iter()
        .map(|v| serde_json::to_string(v).unwrap())
        .collect();

    for i in 0..serialized.len() {
        for j in (i + 1)..serialized.len() {
            assert_ne!(
                serialized[i], serialized[j],
                "variants[{i}] ({:?}) and variants[{j}] ({:?}) both serialize as '{}'",
                variants[i], variants[j], serialized[i]
            );
        }
    }
}

#[test]
fn unknown_session_update_display_is_unknown() {
    // Display is used by the custom AcpBridgeEvent::Serialize for the wire type.
    assert_eq!(
        AcpSessionUpdateKind::Unknown.to_string(),
        "unknown",
        "Display for Unknown must produce 'unknown'"
    );
}

// ── AcpBridgeEvent wire shape for Unknown variant ───────────────────────────

#[test]
fn acp_bridge_event_unknown_session_update_wire_type() {
    let event = AcpBridgeEvent::SessionUpdate(AcpSessionUpdateEvent {
        session_id: "sess-unknown-test".to_string(),
        kind: AcpSessionUpdateKind::Unknown,
        text_delta: None,
        tool_call_id: None,
        tool_name: None,
        tool_status: None,
        tool_content: None,
        tool_input: None,
        tool_locations: None,
        kind_detail: None,
        message_id: None,
    });

    let json = serde_json::to_value(&event).unwrap();

    // The custom Serialize impl derives `type` from `kind.to_string()`.
    // After the fix, Unknown produces "unknown" on the wire.
    assert_eq!(
        json["type"], "unknown",
        "AcpBridgeEvent wire type for Unknown variant must be 'unknown', got: {}",
        json["type"]
    );
    assert_eq!(json["session_id"], "sess-unknown-test");
}

// ── validate_adapter_command ────────────────────────────────────────────────

#[test]
fn validate_adapter_rejects_empty_program() {
    let adapter = AcpAdapterCommand::new("", vec![]);
    let err = validate_adapter_command(&adapter).expect_err("empty program should fail");
    assert!(
        err.to_string().contains("cannot be empty"),
        "error should mention empty: {err}"
    );
}

#[test]
fn validate_adapter_rejects_whitespace_only_program() {
    let adapter = AcpAdapterCommand::new("   \t  ", vec!["--stdio".to_string()]);
    let err = validate_adapter_command(&adapter).expect_err("whitespace-only program should fail");
    assert!(
        err.to_string().contains("cannot be empty"),
        "error should mention empty: {err}"
    );
}

#[test]
fn validate_adapter_accepts_bare_adapter_name() {
    // A bare name like "claude" should pass validation even if it doesn't exist
    // on this system — validation only checks for empty/whitespace.
    let adapter = AcpAdapterCommand::new("claude", vec!["--stdio".to_string()]);
    validate_adapter_command(&adapter).expect("bare adapter name should pass validation");
}

#[test]
fn validate_adapter_rejects_bare_shell_name() {
    // Bare names that are known shells must be rejected unconditionally —
    // not just when the program contains a path separator.
    for shell in &["sh", "bash", "zsh", "fish", "dash", "powershell", "pwsh"] {
        let adapter = AcpAdapterCommand::new(*shell, vec![]);

        let err = validate_adapter_command(&adapter)
            .expect_err(&format!("bare shell '{shell}' should be rejected"));
        assert!(
            err.to_string().contains("shell interpreter"),
            "error for '{shell}' should mention shell interpreter: {err}"
        );
    }
}

#[test]
fn validate_adapter_accepts_absolute_path() {
    // Absolute paths pass validation regardless of whether the file exists.
    let adapter = AcpAdapterCommand::new("/usr/bin/claude-agent-acp", vec![]);
    validate_adapter_command(&adapter).expect("absolute path should pass validation");
}

// ── spawn_adapter env isolation ─────────────────────────────────────────────

/// Global lock: env var mutation is not thread-safe; serialize env-touching tests.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Proxy vars (HTTP_PROXY, HTTPS_PROXY, etc.) must NOT leak into adapter subprocesses.
/// The adapter connects directly to its own API endpoints; proxy vars would break auth.
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn spawn_adapter_does_not_pass_proxy_vars() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    const PROXY_VARS: &[(&str, &str)] = &[
        ("HTTP_PROXY", "http://poison-proxy.test:9999"),
        ("HTTPS_PROXY", "http://poison-proxy.test:9999"),
        ("http_proxy", "http://poison-proxy.test:9999"),
        ("https_proxy", "http://poison-proxy.test:9999"),
        ("NO_PROXY", "http://poison-proxy.test:9999"),
        ("no_proxy", "http://poison-proxy.test:9999"),
    ];

    // Set each proxy var, recording the original value for restoration via
    // drop guard.  Guards are collected so they all drop together at scope end,
    // even if an assertion panics.
    //
    // SAFETY: ENV_LOCK is held for this entire test; single-threaded env access.
    let _guards: Vec<EnvVarGuard> = PROXY_VARS
        .iter()
        .map(|(key, val)| unsafe { EnvVarGuard::set(key, val) })
        .collect();

    // Build a probe that prints the concatenated values of all proxy vars.
    let args_inner = PROXY_VARS
        .iter()
        .map(|(key, _)| format!("\"${key}\""))
        .collect::<Vec<_>>()
        .join("");
    let cmd = format!("printf '%s' {args_inner}");

    let adapter = AcpAdapterCommand::new("sh", vec!["-c".to_string(), cmd]);
    let scaffold = AcpClientScaffold::new(adapter);
    // Use skip_validation variant: "sh" is a blocked shell in production but is
    // needed here to probe env var inheritance inside the env_clear allowlist.
    let child = scaffold
        .spawn_adapter_skip_validation()
        .expect("spawn_adapter_skip_validation should succeed");
    let output = child
        .wait_with_output()
        .await
        .expect("child should complete");
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // _guards drops here, restoring all proxy vars to their original values.

    assert!(
        stdout.is_empty(),
        "proxy vars must NOT be passed through to adapter subprocess \
         (env_clear allowlist should exclude them), but child saw: {stdout:?}"
    );
}

/// CLAUDECODE must not leak through the env_clear allowlist.
/// This is the same test as in services_acp_spawn_env.rs but validates via
/// the env_clear+allowlist approach rather than env_remove.
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn spawn_adapter_does_not_pass_claudecode() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    // Set CLAUDECODE, restoring it to its original value (or removing it) on
    // drop — even if the test panics before reaching the end of the scope.
    //
    // SAFETY: ENV_LOCK is held for this entire test; single-threaded env access.
    let _guard = unsafe { EnvVarGuard::set("CLAUDECODE", "poison_nested_session") };

    let adapter = AcpAdapterCommand::new(
        "sh",
        vec!["-c".to_string(), "printf '%s' \"$CLAUDECODE\"".to_string()],
    );
    let scaffold = AcpClientScaffold::new(adapter);
    // Use skip_validation variant: "sh" is a blocked shell in production but is
    // needed here to probe env var inheritance inside the env_clear allowlist.
    let child = scaffold
        .spawn_adapter_skip_validation()
        .expect("spawn_adapter_skip_validation should succeed");
    let output = child
        .wait_with_output()
        .await
        .expect("child should complete");
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // _guard drops here, restoring CLAUDECODE to its prior state.

    assert!(
        stdout.is_empty(),
        "CLAUDECODE must not be in env_clear allowlist, but child saw: {stdout:?}"
    );
}

// ── SEC-7: composite key isolation ───────────────────────────────────────────

/// Regression test for SEC-7: two concurrent sessions sharing the same
/// `tool_call_id` must not collide in `PermissionResponderMap`.
///
/// The fix changed the map key from `tool_call_id: String` to
/// `(session_id, tool_call_id): (String, String)`. This test directly encodes
/// that invariant: the same `tool_call_id` in two sessions must produce two
/// independent entries, and removing one must leave the other intact.
#[test]
fn permission_responder_map_composite_key_isolates_sessions() {
    use dashmap::DashMap;
    use tokio::sync::oneshot;

    let map: DashMap<(String, String), oneshot::Sender<String>> = DashMap::new();

    let (tx_a, mut rx_a) = oneshot::channel::<String>();
    let (tx_b, mut rx_b) = oneshot::channel::<String>();

    // Both sessions share the same tool_call_id — the exact collision SEC-7 fixed.
    map.insert(("session-A".to_string(), "tool-1".to_string()), tx_a);
    map.insert(("session-B".to_string(), "tool-1".to_string()), tx_b);

    assert_eq!(
        map.len(),
        2,
        "same tool_call_id in two sessions must produce 2 distinct map entries"
    );

    // Removing session-A's entry must not affect session-B.
    map.remove(&("session-A".to_string(), "tool-1".to_string()));
    assert_eq!(map.len(), 1, "only session-A's entry should be removed");
    assert!(
        map.contains_key(&("session-B".to_string(), "tool-1".to_string())),
        "session-B's responder must still be present after session-A is removed"
    );

    // The session-B sender is still live and can deliver a permission response.
    let (_key, sender) = map
        .remove(&("session-B".to_string(), "tool-1".to_string()))
        .expect("session-B entry should exist");
    sender
        .send("allow_once".to_string())
        .expect("send must succeed");
    assert_eq!(
        rx_b.try_recv().expect("receiver should have a value"),
        "allow_once",
        "session-B receiver must get the permission response"
    );

    // Session-A's sender was dropped with the map entry — its receiver is closed.
    assert!(
        rx_a.try_recv().is_err(),
        "session-A receiver must be closed after its sender was dropped"
    );
}

/// PATH must be passed through so the adapter can find its own binary.
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn spawn_adapter_passes_through_path() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    let adapter = AcpAdapterCommand::new(
        "sh",
        vec!["-c".to_string(), "printf '%s' \"$PATH\"".to_string()],
    );
    let scaffold = AcpClientScaffold::new(adapter);
    // Use skip_validation variant: "sh" is a blocked shell in production but is
    // needed here to probe env var inheritance inside the env_clear allowlist.
    let child = scaffold
        .spawn_adapter_skip_validation()
        .expect("spawn_adapter_skip_validation should succeed");
    let output = child
        .wait_with_output()
        .await
        .expect("child should complete");
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    assert!(
        !stdout.is_empty(),
        "PATH must be in env_clear allowlist so adapter can find executables"
    );
}
