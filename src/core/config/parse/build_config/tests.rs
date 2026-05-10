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
pub(super) use std::path::Path;
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

#[allow(unsafe_code)]
#[test]
fn empty_output_dir_env_falls_through_to_default_data_dir_output() {
    let _guard = ENV_LOCK.lock().unwrap();
    with_env_saved(&["AXON_OUTPUT_DIR", "AXON_DATA_DIR"], || unsafe {
        env::set_var("AXON_OUTPUT_DIR", "");
        env::remove_var("AXON_DATA_DIR");

        let cfg = into_config(cli_with_services(&["crawl", "https://example.com"]))
            .expect("empty AXON_OUTPUT_DIR should not fail clap/config parsing");

        assert_eq!(
            cfg.output_dir,
            crate::core::paths::axon_data_base_dir().join("output")
        );
    });
}

#[allow(unsafe_code)]
#[test]
fn empty_sqlite_path_env_falls_through_to_default_jobs_db() {
    let _guard = ENV_LOCK.lock().unwrap();
    with_env_saved(&["AXON_SQLITE_PATH", "AXON_DATA_DIR"], || unsafe {
        env::set_var("AXON_SQLITE_PATH", "");
        env::remove_var("AXON_DATA_DIR");

        let cfg = into_config(cli_with_services(&["crawl", "https://example.com"]))
            .expect("empty AXON_SQLITE_PATH should not produce an empty database path");

        assert_eq!(
            cfg.sqlite_path,
            crate::core::paths::axon_data_base_dir().join("jobs.db")
        );
    });
}

#[allow(unsafe_code)]
#[test]
fn nonempty_output_dir_env_overrides_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    with_env_saved(&["AXON_OUTPUT_DIR"], || unsafe {
        env::set_var("AXON_OUTPUT_DIR", "/tmp/axon-output-from-env");

        let cfg = into_config(cli_with_services(&["crawl", "https://example.com"]))
            .expect("non-empty AXON_OUTPUT_DIR should parse");

        assert_eq!(cfg.output_dir, Path::new("/tmp/axon-output-from-env"));
    });
}

#[allow(unsafe_code)]
#[test]
fn output_dir_flag_wins_over_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    with_env_saved(&["AXON_OUTPUT_DIR"], || unsafe {
        env::set_var("AXON_OUTPUT_DIR", "/tmp/axon-output-from-env");

        let cfg = into_config(cli_with_services(&[
            "--output-dir",
            "/tmp/axon-output-from-flag",
            "crawl",
            "https://example.com",
        ]))
        .expect("--output-dir flag should parse");

        assert_eq!(cfg.output_dir, Path::new("/tmp/axon-output-from-flag"));
    });
}
