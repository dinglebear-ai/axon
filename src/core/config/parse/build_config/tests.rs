//! Tests for `build_config::into_config()`.
//!
//! Split into two themed submodules (bead axon_rust-2j9.6):
//!   * `lite_mode`        — AXON_LITE / MCP origin / URL-required env tests
//!   * `priority_chain`   — `CLI > env > TOML > default` tests for ask/hybrid/tei/workers/search
//!
//! Test BODIES are unchanged from the previous flat `mod tests` in `build_config.rs`.
//! Shared fixtures (`ENV_LOCK`, `cli_with_services`, `with_env_saved`) live here so
//! both submodules can reference them via `super::*`.

mod lite_mode;
mod priority_chain;

pub(super) use super::into_config;
pub(super) use crate::core::config::cli::Cli;
pub(super) use crate::core::config::parse::docker::normalize_local_service_url;
pub(super) use clap::Parser;
pub(super) use std::env;
pub(super) use std::io::Write as _;
pub(super) use std::sync::Mutex;
pub(super) use tempfile::Builder as TempfileBuilder;

pub(super) static ENV_LOCK: Mutex<()> = Mutex::new(());

// Convenience: build a CLI with stable service URLs via flags (avoids QDRANT_URL/TEI_URL env noise).
pub(super) fn cli_with_services(extra: &[&str]) -> Cli {
    let mut args = vec![
        "axon",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "--tei-url",
        "http://127.0.0.1:52000",
    ];
    args.extend_from_slice(extra);
    Cli::parse_from(args)
}

/// Save/restore an env var around a test body so panics don't leak state.
#[allow(unsafe_code)]
pub(super) fn with_env_saved<F: FnOnce()>(keys: &[&str], body: F) {
    let saved: Vec<(String, Option<String>)> = keys
        .iter()
        .map(|k| ((*k).to_string(), env::var(k).ok()))
        .collect();
    body();
    for (k, v) in saved {
        unsafe {
            match v {
                Some(val) => env::set_var(&k, val),
                None => env::remove_var(&k),
            }
        }
    }
}
