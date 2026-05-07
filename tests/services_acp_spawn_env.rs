// std::env::set_var / remove_var are unsafe in Rust 1.81+ (POSIX multi-thread constraint).
// These integration tests are the only place in the codebase that need this, and they
// hold a process-level Mutex to serialize access. Package-level `deny(unsafe_code)`
// is overridden at the file level here, which is permitted because `deny` (unlike
// `forbid`) can be narrowed by a child `allow`.
#![allow(unsafe_code)]

/// Regression tests: spawn_adapter() must strip specific env vars from the child.
///
/// Background:
/// - Claude Code sets `CLAUDECODE=1` in every child process it spawns.
/// - When axon runs inside a Claude Code session (local dev, pre-commit hooks),
///   `claude-agent-acp` inherits `CLAUDECODE` and the inner `claude` CLI detects
///   a nested session, printing "Claude Code cannot be launched inside another
///   Claude Code session" and exiting 1. This was the root cause of the
///   "Query closed before response received" error in Pulse Chat.
/// - `OPENAI_BASE_URL`, `OPENAI_API_KEY`, `OPENAI_MODEL` point at Axon's local LLM
///   proxy. If inherited, the claude/codex adapters would try to use the wrong
///   endpoint and authentication scheme.
///
/// Fix: `spawn_adapter()` calls `command.env_remove()` for each of these vars.
///
/// These tests inject poison values into the current process env, spawn a child
/// command that would expose those values if inherited, and assert the output is
/// empty. They use a process-level mutex so env mutations don't race with other
/// tests in the same binary.
use axon::services::acp::AcpClientScaffold;
use axon::services::types::AcpAdapterCommand;
use std::collections::HashMap;
use std::sync::Mutex;

/// Global lock: env var mutation is not thread-safe; serialize these tests.
static ENV_LOCK: Mutex<()> = Mutex::new(());

const ENV_BIN: &str = "/usr/bin/env";

/// Vars that spawn_adapter() MUST strip before exec-ing the adapter subprocess.
const STRIPPED_VARS: &[&str] = &[
    "CLAUDECODE",
    "OPENAI_BASE_URL",
    "OPENAI_API_KEY",
    "OPENAI_MODEL",
];

fn parse_env(stdout: &[u8]) -> HashMap<String, String> {
    String::from_utf8_lossy(stdout)
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// Spawn a validated env probe and return the child environment as key/value pairs.
async fn run_env_probe() -> HashMap<String, String> {
    let adapter = AcpAdapterCommand::new(ENV_BIN, vec![]);
    let scaffold = AcpClientScaffold::new(adapter);
    let child = scaffold
        .spawn_adapter()
        .expect("spawn_adapter should succeed for /usr/bin/env");
    let output = child
        .wait_with_output()
        .await
        .expect("child should complete");
    parse_env(&output.stdout)
}

/// Spawn a validated env probe and return just the requested values.
async fn run_selected_env_probe(vars: &[&str]) -> Vec<Option<String>> {
    let env = run_env_probe().await;
    vars.iter().map(|key| env.get(*key).cloned()).collect()
}

/// Returns true when all STRIPPED_VARS were absent from the child env.
async fn stripped_vars_are_absent() -> bool {
    run_selected_env_probe(STRIPPED_VARS)
        .await
        .iter()
        .all(Option::is_none)
}

/// When CLAUDECODE is set in the parent environment, the child must NOT inherit it.
///
/// Regression: if `command.env_remove("CLAUDECODE")` is removed from spawn_adapter(),
/// this test fails whenever it runs inside a Claude Code session (which pre-commit
/// hooks do by definition).
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn spawn_adapter_strips_claudecode_nested_session_guard() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // SAFETY: ENV_LOCK is held; no concurrent env mutation in this process.
    unsafe { std::env::set_var("CLAUDECODE", "test_poison_nested_session") };

    let output = stripped_vars_are_absent().await;

    // SAFETY: ENV_LOCK is held.
    unsafe { std::env::remove_var("CLAUDECODE") };
    assert!(
        output,
        "CLAUDECODE must be stripped from child env by spawn_adapter(), \
         but child still saw it"
    );
}

/// When Axon's LLM proxy vars are set in the parent, the child must NOT inherit them.
///
/// ACP adapters authenticate directly (OAuth / API keys stored in ~/.claude or
/// ~/.codex). If the Axon proxy vars leak in, the adapter calls the wrong endpoint.
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn spawn_adapter_strips_llm_proxy_vars() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // SAFETY: ENV_LOCK is held; no concurrent env mutation in this process.
    unsafe {
        std::env::set_var("OPENAI_BASE_URL", "http://poison.axon-proxy.test/v1");
        std::env::set_var("OPENAI_API_KEY", "sk-poison-axon-proxy-test");
        std::env::set_var("OPENAI_MODEL", "poison-axon-model");
    }

    let output = stripped_vars_are_absent().await;

    // SAFETY: ENV_LOCK is held.
    unsafe {
        std::env::remove_var("OPENAI_BASE_URL");
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("OPENAI_MODEL");
    }
    assert!(
        output,
        "OPENAI_* proxy vars must be stripped from child env by spawn_adapter(), \
         but child still saw one or more values"
    );
}

/// Gemini auth vars (GEMINI_API_KEY, GOOGLE_API_KEY) must be passed through to the child,
/// not stripped. These are needed for Gemini CLI authentication.
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn spawn_adapter_passes_through_gemini_auth_vars() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    const GEMINI_VARS: &[&str] = &["GEMINI_API_KEY", "GOOGLE_API_KEY"];
    const SENTINEL: &str = "gemini_test_sentinel";

    // SAFETY: ENV_LOCK is held; no concurrent env mutation in this process.
    unsafe {
        for v in GEMINI_VARS {
            std::env::set_var(v, SENTINEL);
        }
    }

    let values = run_selected_env_probe(GEMINI_VARS).await;

    // SAFETY: ENV_LOCK is held.
    unsafe {
        for v in GEMINI_VARS {
            std::env::remove_var(v);
        }
    }

    assert_eq!(
        values,
        vec![Some(SENTINEL.to_string()), Some(SENTINEL.to_string())],
        "GEMINI_API_KEY and GOOGLE_API_KEY must be passed through to child env"
    );
}

/// All STRIPPED_VARS injected simultaneously — none must leak through.
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn spawn_adapter_strips_all_isolation_vars_together() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // SAFETY: ENV_LOCK is held; no concurrent env mutation in this process.
    unsafe {
        std::env::set_var("CLAUDECODE", "1");
        std::env::set_var("OPENAI_BASE_URL", "http://poison.test/v1");
        std::env::set_var("OPENAI_API_KEY", "sk-poison");
        std::env::set_var("OPENAI_MODEL", "poison");
    }

    let output = stripped_vars_are_absent().await;

    // SAFETY: ENV_LOCK is held.
    unsafe {
        for v in STRIPPED_VARS {
            std::env::remove_var(v);
        }
    }
    assert!(output, "All isolation vars must be stripped together");
}
