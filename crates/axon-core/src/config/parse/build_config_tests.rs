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
pub(super) use crate::config::cli::Cli;
pub(super) use crate::config::parse::docker::normalize_local_service_url;
pub(super) use crate::config::types::Config;
pub(super) use clap::{CommandFactory, FromArgMatches, Parser, parser::ValueSource};
pub(super) use std::env;
pub(super) use std::io::Write as _;
pub(super) use std::path::Path;
pub(super) use std::sync::Mutex;
pub(super) use tempfile::Builder as TempfileBuilder;

/// Process-wide lock serializing all `std::env` mutation across axon-core's
/// test suite. `std::env` is process-global, so ANY test module that mutates
/// env vars (directly or via `AXON_CONFIG_PATH`) MUST acquire this lock via
/// [`env_guard()`] rather than declaring a local/private lock of its own — a
/// second, same-named-but-distinct `Mutex` elsewhere provides zero mutual
/// exclusion with this one and reintroduces the exact cross-module race this
/// lock exists to prevent (see `health::doctor::config_checks_tests`, which
/// reuses this lock instead of a private one).
pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Acquire [`ENV_LOCK`] and isolate `AXON_CONFIG_PATH` to a shared empty temp
/// file for the duration of the test run, so tests exercise only the CLI/env/
/// default layers by default regardless of any real `~/.axon/config.toml` on
/// the host running the suite. Tests that need a specific TOML fixture still
/// override `AXON_CONFIG_PATH` afterward exactly as before.
///
/// This is the ONLY sanctioned way to mutate `AXON_CONFIG_PATH` (or any other
/// process-global env var) in axon-core tests — every test module in the
/// crate that touches env vars must call this instead of rolling its own
/// lock, or concurrent `cargo test` runs will race and poison each other.
#[allow(unsafe_code)]
pub(crate) fn env_guard() -> std::sync::MutexGuard<'static, ()> {
    static ISOLATED_CONFIG: std::sync::OnceLock<tempfile::NamedTempFile> =
        std::sync::OnceLock::new();
    let guard = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let path = ISOLATED_CONFIG.get_or_init(|| {
        TempfileBuilder::new()
            .suffix(".toml")
            .tempfile()
            .expect("create isolated empty config.toml for tests")
    });
    unsafe {
        env::set_var("AXON_CONFIG_PATH", path.path());
    }
    guard
}

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
fn monitor_jobs_watch_is_accepted() {
    let _guard = env_guard();
    // `--watch` is a `global = true` flag; `monitor jobs --watch` must parse
    // without the "only supported with axon status" guard rejecting it.
    let cfg = into_config_via_args(&["monitor", "jobs", "--watch"])
        .expect("monitor jobs --watch should be accepted");
    assert_eq!(cfg.command, crate::config::types::CommandKind::Monitor);
    assert!(cfg.watch_mode, "global --watch should populate watch_mode");
}

#[test]
fn watch_rejected_for_non_status_non_monitor_command() {
    let _guard = env_guard();
    let err = into_config_via_args(&["stats", "--watch"])
        .expect_err("global --watch on a plain command should be rejected");
    assert!(
        err.contains("--watch is only supported"),
        "unexpected error: {err}"
    );
}

#[allow(unsafe_code)]
#[test]
fn extract_and_crawl_defaults_are_bounded_but_explicit_zero_stays_uncapped() {
    let _guard = env_guard();
    let mut default_toml = TempfileBuilder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp default config");
    writeln!(default_toml).expect("write empty config");

    with_env_saved(
        &[
            "AXON_CONFIG_PATH",
            "AXON_ALLOW_UNBOUNDED_BROAD_CRAWL",
            "AXON_CRAWL_MEMORY_ABORT_PERCENT",
        ],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", default_toml.path());
            env::remove_var("AXON_ALLOW_UNBOUNDED_BROAD_CRAWL");
            env::remove_var("AXON_CRAWL_MEMORY_ABORT_PERCENT");

            // `extract` (the surviving URL command) defaults to the single-page cap.
            let default_extract = into_config_via_args(&["extract", "https://example.com/page"])
                .expect("extract config should parse");
            assert_eq!(default_extract.max_pages, 1);
            assert_eq!(
                default_extract.max_page_bytes,
                Some(crate::config::types::DEFAULT_MAX_PAGE_BYTES)
            );
            assert_eq!(
                default_extract.crawl_broadcast_buffer_min,
                crate::config::types::DEFAULT_CRAWL_BROADCAST_BUFFER_MIN
            );
            assert_eq!(
                default_extract.crawl_broadcast_buffer_max,
                crate::config::types::DEFAULT_CRAWL_BROADCAST_BUFFER_MAX
            );

            // An explicit `--max-pages 0` stays uncapped (the services layer fills
            // in any crawl default; the parse layer treats `0` as "unspecified").
            let explicit_uncapped =
                into_config_via_args(&["--max-pages", "0", "extract", "https://example.com/page"])
                    .expect("extract config with explicit max-pages should parse");
            assert_eq!(explicit_uncapped.max_pages, 0);

            let mut unlimited_toml = TempfileBuilder::new()
                .suffix(".toml")
                .tempfile()
                .expect("temp unlimited config");
            writeln!(unlimited_toml, "[crawl]\nmax-page-bytes = 0")
                .expect("write unlimited config");
            env::set_var("AXON_CONFIG_PATH", unlimited_toml.path());
            let explicit_unlimited_bytes =
                into_config_via_args(&["extract", "https://example.com"])
                    .expect("crawl config with explicit max-page-bytes should parse");
            assert_eq!(explicit_unlimited_bytes.max_page_bytes, None);

            env::set_var("AXON_CONFIG_PATH", default_toml.path());
            env::set_var("AXON_ALLOW_UNBOUNDED_BROAD_CRAWL", "true");
            env::set_var("AXON_CRAWL_MEMORY_ABORT_PERCENT", "0");
            let env_overrides = into_config_via_args(&["extract", "https://example.com"])
                .expect("crawl config with env memory knobs should parse");
            assert!(env_overrides.allow_unbounded_broad_crawl);
            assert_eq!(env_overrides.crawl_memory_abort_percent, None);
        },
    );
}

/// Save/restore env vars around a test body so panics don't leak state.
#[allow(unsafe_code)]
pub(in crate::config::parse) fn with_env_saved<F: FnOnce()>(keys: &[&str], body: F) {
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
fn source_subcommand_parses_local_path_positional() {
    // into_config() unconditionally reads AXON_COLLECTION/AXON_SQLITE_PATH/
    // AXON_OUTPUT_DIR and (via load_toml_config()) AXON_CONFIG_PATH for the
    // `source` command -- it is not in into_config's early-return command
    // list that skips env/TOML reads. Without ENV_LOCK this test can run
    // concurrently with another test in this file that mutates those same
    // env vars (most of them do hold the lock), racing on process-global
    // env state and occasionally observing a torn/unexpected value.
    let _guard = env_guard();

    let cfg = into_config(cli_with_services(&["source", "./somepath"]))
        .expect("source subcommand should parse");

    assert_eq!(cfg.command, crate::config::types::CommandKind::Source);
    assert_eq!(cfg.positional, vec!["./somepath".to_string()]);
}

#[allow(unsafe_code)]
#[test]
fn source_subcommand_without_path_has_empty_positional() {
    // See source_subcommand_parses_local_path_positional above for why this
    // needs ENV_LOCK: into_config() reads process env for the `source`
    // command, and other tests in this file mutate that same env under the
    // lock.
    let _guard = env_guard();

    let cfg = into_config(cli_with_services(&["source"]))
        .expect("source subcommand without a path should parse");

    assert_eq!(cfg.command, crate::config::types::CommandKind::Source);
    assert!(cfg.positional.is_empty());
}

#[allow(unsafe_code)]
#[test]
fn skip_embed_flag_disables_default_embedding() {
    let _guard = env_guard();

    let cfg = into_config(cli_with_services(&[
        "--skip-embed",
        "extract",
        "https://example.com",
    ]))
    .expect("--skip-embed should parse");

    assert!(!cfg.embed);
}

#[allow(unsafe_code)]
#[test]
fn empty_output_dir_env_falls_through_to_default_data_dir_output() {
    let _guard = env_guard();
    with_env_saved(&["AXON_OUTPUT_DIR", "AXON_DATA_DIR"], || unsafe {
        env::set_var("AXON_OUTPUT_DIR", "");
        env::remove_var("AXON_DATA_DIR");

        let cfg = into_config(cli_with_services(&["extract", "https://example.com"]))
            .expect("empty AXON_OUTPUT_DIR should not fail clap/config parsing");

        assert_eq!(
            cfg.output_dir,
            crate::paths::axon_data_base_dir().join("output")
        );
    });
}

#[allow(unsafe_code)]
#[test]
fn empty_sqlite_path_env_falls_through_to_default_jobs_db() {
    let _guard = env_guard();
    with_env_saved(&["AXON_SQLITE_PATH", "AXON_DATA_DIR"], || unsafe {
        env::set_var("AXON_SQLITE_PATH", "");
        env::remove_var("AXON_DATA_DIR");

        let cfg = into_config(cli_with_services(&["extract", "https://example.com"]))
            .expect("empty AXON_SQLITE_PATH should not produce an empty database path");

        assert_eq!(
            cfg.sqlite_path,
            crate::paths::axon_data_base_dir().join("jobs.db")
        );
    });
}

#[allow(unsafe_code)]
#[test]
fn reset_vectors_reads_qdrant_env_without_requiring_tei() {
    let _guard = env_guard();
    with_env_saved(&["QDRANT_URL", "TEI_URL"], || unsafe {
        env::set_var("QDRANT_URL", "http://198.51.100.5:53333");
        env::remove_var("TEI_URL");

        let cfg = into_config(Cli::parse_from([
            "axon",
            "reset",
            "--dry-run",
            "--stores",
            "vectors",
        ]))
        .expect("reset vectors should resolve Qdrant without requiring TEI");

        assert_eq!(cfg.command, crate::config::types::CommandKind::Reset);
        assert_eq!(cfg.qdrant_url, "http://198.51.100.5:53333");
        assert_eq!(cfg.collection, "axon");
    });
}

#[allow(unsafe_code)]
#[test]
fn nonempty_output_dir_env_overrides_default() {
    let _guard = env_guard();
    with_env_saved(&["AXON_OUTPUT_DIR"], || unsafe {
        env::set_var("AXON_OUTPUT_DIR", "/tmp/axon-output-from-env");

        let cfg = into_config(cli_with_services(&["extract", "https://example.com"]))
            .expect("non-empty AXON_OUTPUT_DIR should parse");

        assert_eq!(cfg.output_dir, Path::new("/tmp/axon-output-from-env"));
    });
}

#[allow(unsafe_code)]
#[test]
fn output_dir_flag_wins_over_env() {
    let _guard = env_guard();
    with_env_saved(&["AXON_OUTPUT_DIR"], || unsafe {
        env::set_var("AXON_OUTPUT_DIR", "/tmp/axon-output-from-env");

        let cfg = into_config(cli_with_services(&[
            "--output-dir",
            "/tmp/axon-output-from-flag",
            "extract",
            "https://example.com",
        ]))
        .expect("--output-dir flag should parse");

        assert_eq!(cfg.output_dir, Path::new("/tmp/axon-output-from-flag"));
    });
}

#[allow(unsafe_code)]
#[test]
fn explicit_default_output_dir_flag_wins_over_env() {
    let _guard = env_guard();
    with_env_saved(&["AXON_OUTPUT_DIR"], || unsafe {
        env::set_var("AXON_OUTPUT_DIR", "/tmp/axon-output-from-env");

        let (cli, output_dir_was_explicit, collection_was_explicit) =
            cli_with_services_and_sources(&[
                "--output-dir",
                crate::config::cli::DEFAULT_OUTPUT_DIR,
                "extract",
                "https://example.com",
            ]);
        let cfg = into_config_with_sources(cli, output_dir_was_explicit, collection_was_explicit)
            .expect("explicit default --output-dir should parse");

        assert_eq!(
            cfg.output_dir,
            Path::new(crate::config::cli::DEFAULT_OUTPUT_DIR)
        );
    });
}

#[allow(unsafe_code)]
#[test]
fn migrated_crawl_tuning_reads_from_toml() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[crawl]\nrespect-robots = true\nmin-markdown-chars = 777\ndrop-thin-markdown = false\ndiscover-sitemaps = false\nsitemap-since-days = 9\nmax-sitemaps = 42\nauto-switch-thin-ratio = 0.25\nauto-switch-min-pages = 3\nurl-whitelist = [\"^https://example.com/docs\"]\nmax-page-bytes = 9999\nredirect-policy-strict = true\n\n[providers.fetch]\ndelay-ms = 123\nrequest-timeout-ms = 4567\nretries = 5\nretry-backoff-ms = 321\n\n[providers.render]\nbypass-csp = true\naccept-invalid-certs = true\nnetwork-idle-timeout-secs = 22\n"
    )
    .unwrap();

    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        let cfg = into_config_via_args(&["extract", "https://example.com"]).unwrap();

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
fn migrated_embed_openai_tuning_reads_from_toml_and_env_still_wins() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[providers.embedding]\nmax-concurrent-requests = 7\nmax-in-flight-inputs = 240\ncache-enabled = true\ncache-max-entries = 250000\npool-max-inputs = 640\nscheduler-enabled = false\nvector-upsert-overlap-enabled = false\nprep-concurrency = 3\nmax-chunks-per-doc = 50\nmax-source-chunks-per-doc = 75\ndedupe-exact-chunks = false\nopenai-model = \"from-toml\"\nopenai-max-client-batch-size = 24\nopenai-max-concurrent = 12\nopenai-max-in-flight-inputs = 256\nopenai-pool-max-inputs = 768\n"
    )
    .unwrap();

    with_env_saved(
        &[
            "AXON_CONFIG_PATH",
            "AXON_OPENAI_EMBED_MAX_CONCURRENT",
            "AXON_OPENAI_EMBEDDING_MODEL",
            "AXON_EMBED_MAX_SOURCE_CHUNKS_PER_DOC",
            "AXON_EMBED_CACHE_ENABLED",
            "AXON_EMBED_CACHE_MAX_ENTRIES",
            "AXON_EMBED_SCHEDULER_ENABLED",
            "AXON_VECTOR_UPSERT_EMBED_OVERLAP",
        ],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("AXON_OPENAI_EMBED_MAX_CONCURRENT", "16");
            env::set_var("AXON_OPENAI_EMBEDDING_MODEL", "from-env");
            env::set_var("AXON_EMBED_MAX_SOURCE_CHUNKS_PER_DOC", "0");
            env::remove_var("AXON_EMBED_CACHE_ENABLED");
            env::set_var("AXON_EMBED_CACHE_MAX_ENTRIES", "300000");
            env::remove_var("AXON_EMBED_SCHEDULER_ENABLED");
            env::remove_var("AXON_VECTOR_UPSERT_EMBED_OVERLAP");

            let cfg = into_config_via_args(&["extract", "https://example.com"]).unwrap();

            assert_eq!(cfg.embed_tei_max_concurrent, 7);
            assert_eq!(cfg.embed_tei_max_in_flight_inputs, 240);
            assert!(cfg.embed_cache_enabled);
            assert_eq!(cfg.embed_cache_max_entries, 300_000);
            assert_eq!(cfg.embed_pool_max_inputs, 640);
            assert!(!cfg.embed_scheduler_enabled);
            assert!(!cfg.vector_upsert_embed_overlap);
            assert_eq!(cfg.embed_prep_concurrency, 3);
            assert_eq!(cfg.embed_max_chunks_per_doc, Some(50));
            assert_eq!(cfg.embed_max_source_chunks_per_doc, None);
            assert!(!cfg.embed_dedupe_exact_chunks);
            assert_eq!(cfg.openai_embed_model, "from-env");
            assert_eq!(cfg.openai_embed_max_client_batch_size, 24);
            assert_eq!(cfg.openai_embed_max_concurrent, 16);
            assert_eq!(cfg.openai_embed_max_in_flight_inputs, 256);
            assert_eq!(cfg.openai_embed_pool_max_inputs, 768);

            env::set_var("AXON_EMBED_SCHEDULER_ENABLED", "true");
            env::set_var("AXON_VECTOR_UPSERT_EMBED_OVERLAP", "true");
            let overridden = into_config_via_args(&["extract", "https://example.com"]).unwrap();
            assert!(overridden.embed_scheduler_enabled);
            assert!(overridden.vector_upsert_embed_overlap);
        },
    );
}

#[allow(unsafe_code)]
#[test]
fn markdown_chunking_limits_are_resolved_once_into_config() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[pipeline.chunking]\nmarkdown-max-chars = 900\nmarkdown-min-chars = 300\noverlap-chars = 75\n"
    )
    .unwrap();

    with_env_saved(
        &[
            "AXON_CONFIG_PATH",
            "AXON_MARKDOWN_CHUNK_MAX_CHARS",
            "AXON_MARKDOWN_CHUNK_MIN_CHARS",
            "AXON_CHUNK_OVERLAP_CHARS",
        ],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("AXON_MARKDOWN_CHUNK_MAX_CHARS", "800");
            env::remove_var("AXON_MARKDOWN_CHUNK_MIN_CHARS");
            env::remove_var("AXON_CHUNK_OVERLAP_CHARS");

            let cfg = into_config_via_args(&["extract", "https://example.com"]).unwrap();

            assert_eq!(cfg.chunking_markdown_max_chars, 800);
            assert_eq!(cfg.chunking_markdown_min_chars, 300);
            assert_eq!(cfg.chunking_overlap_chars, 75);
        },
    );
}

#[allow(unsafe_code)]
#[test]
fn openai_embed_model_toml_wins_over_vllm_fallback_env() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.embedding]\nopenai-model = \"from-toml\"\n").unwrap();

    with_env_saved(
        &[
            "AXON_CONFIG_PATH",
            "AXON_OPENAI_EMBEDDING_MODEL",
            "VLLM_SERVED_MODEL_NAME",
        ],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_OPENAI_EMBEDDING_MODEL");
            env::set_var("VLLM_SERVED_MODEL_NAME", "from-vllm");

            let cfg = into_config_via_args(&["extract", "https://example.com"]).unwrap();

            assert_eq!(cfg.openai_embed_model, "from-toml");
        },
    );
}

#[allow(unsafe_code)]
#[test]
fn parses_llms_txt_scrape_keys() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[crawl]\ndiscover-llms-txt = false\nmax-llms-txt-urls = 42\n"
    )
    .unwrap();

    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        let cfg = into_config_via_args(&["extract", "https://example.com"]).unwrap();
        assert!(!cfg.discover_llms_txt);
        assert_eq!(cfg.max_llms_txt_urls, 42);
    });
}

#[allow(unsafe_code)]
#[test]
fn migrated_worker_tuning_reads_from_toml_and_watchdog_env_still_wins() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[crawl]\nconcurrency-limit = 11\n\n[jobs]\nstale-after-secs = 45\nstale-grace-secs = 20\nwatchdog-sweep-secs = 25\n"
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
    let _guard = env_guard();
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
    let _guard = env_guard();
    let mut config = TempfileBuilder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp config");
    writeln!(
        config,
        "[providers.render]\nbootstrap-timeout-ms = 125\nbootstrap-retries = 15"
    )
    .expect("write config");

    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", config.path());

        let cfg = into_config(cli_with_services(&["extract", "https://example.com"]))
            .expect("chrome bootstrap TOML config should parse");

        assert_eq!(cfg.chrome_bootstrap_timeout_ms, 250);
        assert_eq!(cfg.chrome_bootstrap_retries, 10);
    });
}

#[test]
fn crawl_cache_defaults_off() {
    let _guard = env_guard();
    let cfg = into_config(cli_with_services(&["extract", "https://example.com"]))
        .expect("crawl config should parse");
    assert!(!cfg.cache, "crawl cache must be opt-in");
}

#[test]
fn etag_conditional_without_cache_is_rejected() {
    let _guard = env_guard();
    let result = into_config(cli_with_services(&[
        "--etag-conditional",
        "--cache",
        "false",
        "extract",
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
    let _guard = env_guard();
    let cfg = into_config(cli_with_services(&[
        "--etag-conditional",
        "--cache",
        "true",
        "extract",
        "https://example.com",
    ]))
    .expect("--etag-conditional with explicit --cache true should be valid");
    assert!(cfg.etag_conditional);
    assert!(cfg.cache);
}

#[test]
fn cron_max_runs_requires_an_interval() {
    let _guard = env_guard();
    let err = into_config(cli_with_services(&[
        "--cron-max-runs",
        "2",
        "extract",
        "https://example.com",
    ]))
    .expect_err("a run limit without a schedule must fail");
    assert!(err.contains("--cron-max-runs requires --cron-every-seconds"));
}

#[test]
fn cron_values_reject_zero_instead_of_becoming_unbounded() {
    let _guard = env_guard();
    let interval_err = into_config(cli_with_services(&[
        "--cron-every-seconds",
        "0",
        "extract",
        "https://example.com",
    ]))
    .expect_err("a zero cron interval must fail");
    assert!(interval_err.contains("--cron-every-seconds must be greater than zero"));

    let max_runs_err = into_config(cli_with_services(&[
        "--cron-every-seconds",
        "60",
        "--cron-max-runs",
        "0",
        "extract",
        "https://example.com",
    ]))
    .expect_err("zero max-runs must not silently mean unlimited");
    assert!(max_runs_err.contains("--cron-max-runs must be greater than zero"));
}

#[allow(unsafe_code)]
#[test]
fn projection_batch_env_overrides_toml_and_invalid_limits_fail() {
    let _guard = env_guard();
    let mut file = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        file,
        "[server.projection-batch]\nmax-inputs = 4\nmax-query-window = 40"
    )
    .unwrap();
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_PROJECTION_BATCH_MAX_INPUTS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", file.path());
            env::set_var("AXON_PROJECTION_BATCH_MAX_INPUTS", "2");
            let cfg = into_config_via_args(&["extract", "https://example.com"]).unwrap();
            assert_eq!(cfg.projection_batch.max_inputs, 2);
            assert_eq!(cfg.projection_batch.max_query_window, 40);

            env::set_var("AXON_PROJECTION_BATCH_MAX_INPUTS", "0");
            let error = into_config_via_args(&["extract", "https://example.com"]).unwrap_err();
            assert!(error.contains("projection-batch.max-inputs"));
        },
    );
}
