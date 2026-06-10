//! Tests for `build_config::into_config()`.
//!
//! Split into two themed submodules (bead axon_rust-2j9.6):
//!   * `env_required`     — MCP origin / URL-required env tests
//!   * `priority_chain`   — `CLI > env > TOML > default` tests for ask/hybrid/tei/workers/search
//!
//! Shared fixtures (`ENV_LOCK`, `cli_with_services`, `with_env_saved`) live here so
//! both submodules can reference them via `super::*`.

#[path = "build_config/tests/env_required.rs"]
mod env_required;
#[path = "build_config/tests/priority_chain.rs"]
mod priority_chain;

pub(super) use super::{into_config, into_config_with_sources};
pub(super) use crate::core::config::cli::Cli;
pub(super) use crate::core::config::parse::docker::normalize_local_service_url;
pub(super) use crate::core::config::types::Config;
pub(super) use clap::{CommandFactory, FromArgMatches, Parser, parser::ValueSource};
pub(super) use std::env;
pub(super) use std::io::Write as _;
pub(super) use std::path::Path;
pub(super) use std::sync::Mutex;
pub(super) use tempfile::Builder as TempfileBuilder;

pub(in crate::core::config::parse) static ENV_LOCK: Mutex<()> = Mutex::new(());

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

pub(super) fn cli_with_services_and_sources(extra: &[&str]) -> (Cli, bool, bool) {
    let mut args = vec![
        "axon",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "--tei-url",
        "http://127.0.0.1:52000",
    ];
    args.extend_from_slice(extra);
    let matches = Cli::command().get_matches_from(args);
    let output_dir_was_explicit =
        matches.value_source("output_dir") == Some(ValueSource::CommandLine);
    let collection_was_explicit =
        matches.value_source("collection") == Some(ValueSource::CommandLine);
    let cli = Cli::from_arg_matches(&matches).expect("cli should parse");
    (cli, output_dir_was_explicit, collection_was_explicit)
}

/// Convenience wrapper: parse via clap to recover value_sources, then call
/// `into_config_with_sources`. Use this in tests that need accurate
/// `--collection axon` / explicit-default detection.
pub(super) fn into_config_via_args(extra: &[&str]) -> Result<Config, String> {
    let (cli, output_dir_was_explicit, collection_was_explicit) =
        cli_with_services_and_sources(extra);
    into_config_with_sources(cli, output_dir_was_explicit, collection_was_explicit)
}

#[test]
fn extract_defaults_to_single_page_but_explicit_zero_stays_uncapped() {
    let _guard = ENV_LOCK.lock().unwrap();

    let default_extract = into_config_via_args(&["extract", "https://example.com/page"])
        .expect("extract config should parse");
    assert_eq!(default_extract.max_pages, 1);

    let explicit_uncapped =
        into_config_via_args(&["--max-pages", "0", "extract", "https://example.com/page"])
            .expect("extract config with explicit max-pages should parse");
    assert_eq!(explicit_uncapped.max_pages, 0);

    let default_crawl =
        into_config_via_args(&["crawl", "https://example.com"]).expect("crawl config should parse");
    assert_eq!(default_crawl.max_pages, 0);
}

/// Save/restore env vars around a test body so panics don't leak state.
#[allow(unsafe_code)]
pub(in crate::core::config::parse) fn with_env_saved<F: FnOnce()>(keys: &[&str], body: F) {
    struct EnvRestore {
        saved: Vec<(String, Option<String>)>,
    }

    impl Drop for EnvRestore {
        #[allow(unsafe_code)]
        fn drop(&mut self) {
            for (key, value) in self.saved.drain(..) {
                unsafe {
                    match value {
                        Some(saved) => env::set_var(&key, saved),
                        None => env::remove_var(&key),
                    }
                }
            }
        }
    }

    let _restore = EnvRestore {
        saved: keys
            .iter()
            .map(|k| ((*k).to_string(), env::var(k).ok()))
            .collect(),
    };
    body();
}

#[allow(unsafe_code)]
#[test]
fn skip_embed_flag_disables_default_embedding() {
    let _guard = ENV_LOCK.lock().unwrap();

    let cfg = into_config(cli_with_services(&[
        "--skip-embed",
        "scrape",
        "https://example.com",
    ]))
    .expect("--skip-embed should parse");

    assert!(!cfg.embed);
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

#[allow(unsafe_code)]
#[test]
fn explicit_default_output_dir_flag_wins_over_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    with_env_saved(&["AXON_OUTPUT_DIR"], || unsafe {
        env::set_var("AXON_OUTPUT_DIR", "/tmp/axon-output-from-env");

        let (cli, output_dir_was_explicit, collection_was_explicit) =
            cli_with_services_and_sources(&[
                "--output-dir",
                crate::core::config::cli::DEFAULT_OUTPUT_DIR,
                "crawl",
                "https://example.com",
            ]);
        let cfg = into_config_with_sources(cli, output_dir_was_explicit, collection_was_explicit)
            .expect("explicit default --output-dir should parse");

        assert_eq!(
            cfg.output_dir,
            Path::new(crate::core::config::cli::DEFAULT_OUTPUT_DIR)
        );
    });
}

#[allow(unsafe_code)]
#[test]
fn migrated_crawl_tuning_reads_from_toml() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[scrape]\nrespect-robots = true\nmin-markdown-chars = 777\ndrop-thin-markdown = false\ndiscover-sitemaps = false\nsitemap-since-days = 9\nmax-sitemaps = 42\ndelay-ms = 123\nrequest-timeout-ms = 4567\nfetch-retries = 5\nretry-backoff-ms = 321\nauto-switch-thin-ratio = 0.25\nauto-switch-min-pages = 3\nurl-whitelist = [\"^https://example.com/docs\"]\nmax-page-bytes = 9999\nredirect-policy-strict = true\n\n[chrome]\nbypass-csp = true\naccept-invalid-certs = true\nnetwork-idle-timeout-secs = 22\n"
    )
    .unwrap();

    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        let cfg = into_config_via_args(&["crawl", "https://example.com"]).unwrap();

        assert!(cfg.respect_robots);
        assert_eq!(cfg.min_markdown_chars, 777);
        assert!(!cfg.drop_thin_markdown);
        assert!(!cfg.discover_sitemaps);
        assert_eq!(cfg.sitemap_since_days, 9);
        assert_eq!(cfg.max_sitemaps, 42);
        assert_eq!(cfg.delay_ms, 123);
        assert_eq!(cfg.request_timeout_ms, Some(4567));
        assert_eq!(cfg.fetch_retries, 5);
        assert_eq!(cfg.retry_backoff_ms, 321);
        assert!((cfg.auto_switch_thin_ratio - 0.25).abs() < f64::EPSILON);
        assert_eq!(cfg.auto_switch_min_pages, 3);
        assert_eq!(cfg.url_whitelist, vec!["^https://example.com/docs"]);
        assert_eq!(cfg.max_page_bytes, Some(9999));
        assert!(cfg.redirect_policy_strict);
        assert!(cfg.bypass_csp);
        assert!(cfg.accept_invalid_certs);
        assert_eq!(cfg.chrome_network_idle_timeout_secs, 22);
    });
}

#[allow(unsafe_code)]
#[test]
fn parses_llms_txt_scrape_keys() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[scrape]\ndiscover-llms-txt = false\nmax-llms-txt-urls = 42\n"
    )
    .unwrap();

    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        let cfg = into_config_via_args(&["crawl", "https://example.com"]).unwrap();
        assert!(!cfg.discover_llms_txt);
        assert_eq!(cfg.max_llms_txt_urls, 42);
    });
}

#[allow(unsafe_code)]
#[test]
fn migrated_worker_tuning_reads_from_toml_and_watchdog_env_still_wins() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[workers]\nconcurrency-limit = 11\nwatchdog-stale-timeout-secs = 45\nwatchdog-confirm-secs = 20\nwatchdog-sweep-secs = 25\n"
    )
    .unwrap();

    with_env_saved(
        &[
            "AXON_CONFIG_PATH",
            "AXON_JOB_STALE_TIMEOUT_SECS",
            "AXON_JOB_STALE_CONFIRM_SECS",
            "AXON_WATCHDOG_SWEEP_SECS",
        ],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("AXON_JOB_STALE_TIMEOUT_SECS", "70");
            let cfg = into_config_via_args(&["status"]).unwrap();

            assert_eq!(cfg.crawl_concurrency_limit, Some(11));
            assert_eq!(cfg.backfill_concurrency_limit, Some(11));
            assert_eq!(cfg.watchdog_stale_timeout_secs, 70);
            assert_eq!(cfg.watchdog_confirm_secs, 20);
            assert_eq!(cfg.watchdog_sweep_secs, 25);
        },
    );
}

#[allow(unsafe_code)]
#[test]
fn explicit_default_collection_flag_wins_over_env() {
    // Regression: previously the sentinel check `global.collection != "axon"`
    // treated explicit `--collection axon` the same as the clap default and
    // fell through to env/TOML. With clap value_source threading,
    // `--collection axon` on the CLI must win.
    let _guard = ENV_LOCK.lock().unwrap();
    with_env_saved(&["AXON_COLLECTION"], || unsafe {
        env::set_var("AXON_COLLECTION", "from-env");

        let (cli, output_dir_was_explicit, collection_was_explicit) =
            cli_with_services_and_sources(&["--collection", "axon", "status"]);
        let cfg = into_config_with_sources(cli, output_dir_was_explicit, collection_was_explicit)
            .expect("explicit --collection axon should parse");

        assert_eq!(cfg.collection, "axon");
    });
}

#[allow(unsafe_code)]
#[test]
fn chrome_bootstrap_tuning_comes_from_toml() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut config = TempfileBuilder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp config");
    writeln!(
        config,
        "[chrome]\nbootstrap-timeout-ms = 125\nbootstrap-retries = 15"
    )
    .expect("write config");

    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", config.path());

        let cfg = into_config(cli_with_services(&["crawl", "https://example.com"]))
            .expect("chrome bootstrap TOML config should parse");

        assert_eq!(cfg.chrome_bootstrap_timeout_ms, 250);
        assert_eq!(cfg.chrome_bootstrap_retries, 10);
    });
}

#[test]
fn etag_conditional_without_cache_is_rejected() {
    let _guard = ENV_LOCK.lock().unwrap();
    let result = into_config(cli_with_services(&[
        "--etag-conditional",
        "--cache",
        "false",
        "crawl",
        "https://example.com",
    ]));
    assert!(
        result.is_err(),
        "--etag-conditional with --cache false should be rejected"
    );
    let msg = result.unwrap_err();
    assert!(
        msg.contains("--etag-conditional requires --cache"),
        "error message should explain the requirement, got: {msg}"
    );
}

#[test]
fn etag_conditional_with_cache_true_is_valid() {
    let _guard = ENV_LOCK.lock().unwrap();
    let cfg = into_config(cli_with_services(&[
        "--etag-conditional",
        "crawl",
        "https://example.com",
    ]))
    .expect("--etag-conditional with default --cache true should be valid");
    assert!(cfg.etag_conditional);
    assert!(cfg.cache);
}
