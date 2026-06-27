use super::freshness::FreshDuration;
use super::*;
use std::env;
use std::io::Write as _;

#[allow(unsafe_code)]
fn with_env_saved<F: FnOnce()>(keys: &[&str], body: F) {
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

#[test]
fn test_command_kind_research_as_str() {
    assert_eq!(CommandKind::Research.as_str(), "research");
}

#[test]
fn test_command_kind_screenshot_as_str() {
    assert_eq!(CommandKind::Screenshot.as_str(), "screenshot");
}

#[test]
fn test_command_kind_mcp_as_str() {
    assert_eq!(CommandKind::Mcp.as_str(), "mcp");
}

#[test]
fn test_command_kind_completions_as_str() {
    assert_eq!(CommandKind::Completions.as_str(), "completions");
}

#[test]
fn test_command_kind_update_as_str() {
    assert_eq!(CommandKind::Update.as_str(), "update");
}

#[test]
fn fresh_duration_accepts_whole_days() {
    assert_eq!(FreshDuration::parse("1d").unwrap().days, 1);
    assert_eq!(
        FreshDuration::parse("7d").unwrap().seconds,
        7 * 24 * 60 * 60
    );
    assert_eq!(FreshDuration::parse("366d").unwrap().days, 366);
}

#[test]
fn fresh_duration_rejects_invalid_values() {
    for raw in ["", "0d", "367d", "1h", "24h", "0.5d", "1D", "day", "7"] {
        let err = FreshDuration::parse(raw).unwrap_err();
        assert!(err.contains("--fresh expects a whole-day duration from 1d to 366d"));
    }
}

#[test]
fn config_default_screenshot_settings() {
    let cfg = Config::default();
    assert!(cfg.screenshot_full_page);
    assert_eq!(cfg.viewport_width, 1920);
    assert_eq!(cfg.viewport_height, 1080);
}

#[test]
fn config_default_crawl_settings() {
    let cfg = Config::default();
    assert_eq!(cfg.max_depth, 10);
    assert_eq!(cfg.min_markdown_chars, 200);
    assert!(cfg.discover_sitemaps);
    assert!(cfg.drop_thin_markdown);
    assert!(!cfg.respect_robots);
}

#[test]
fn config_default_vector_settings() {
    let cfg = Config::default();
    assert_eq!(cfg.collection, "axon");
    assert!(cfg.embed);
    assert_eq!(cfg.search_limit, 10);
    assert_eq!(cfg.qdrant_url, "http://127.0.0.1:53333");
}

#[test]
fn config_default_ask_settings() {
    let cfg = Config::default();
    assert_eq!(cfg.ask_max_context_chars, 300_000);
    assert_eq!(cfg.ask_candidate_limit, 250);
    assert!((cfg.ask_min_relevance_score - 0.45).abs() < f64::EPSILON);
    assert!(cfg.ask_authoritative_domains.is_empty());
    assert!((cfg.ask_authoritative_boost - 0.0).abs() < f64::EPSILON);
    assert_eq!(cfg.ask_min_citations_nontrivial, 2);
}

#[test]
fn config_default_worker_settings() {
    let cfg = Config::default();
    assert_eq!(cfg.batch_concurrency, 16);
    assert_eq!(cfg.watchdog_stale_timeout_secs, 300);
    assert_eq!(cfg.watchdog_confirm_secs, 60);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn config_default_ignores_env_tuning_knobs() {
    with_env_saved(
        &[
            "TEI_MAX_RETRIES",
            "AXON_INGEST_LANES",
            "AXON_HNSW_EF_SEARCH",
        ],
        || {
            unsafe {
                env::set_var("TEI_MAX_RETRIES", "19");
                env::set_var("AXON_INGEST_LANES", "12");
                env::set_var("AXON_HNSW_EF_SEARCH", "256");
            }

            let cfg = Config::default();

            assert_eq!(cfg.tei_max_retries, 5);
            assert_eq!(cfg.ingest_lanes, 2);
            assert_eq!(cfg.hnsw_ef_search, 128);
        },
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn config_default_minimal_applies_toml_tuning_when_env_unset() {
    with_env_saved(
        &[
            "AXON_CONFIG_PATH",
            "TEI_MAX_RETRIES",
            "AXON_INGEST_LANES",
            "AXON_HNSW_EF_SEARCH",
        ],
        || {
            let mut file = tempfile::Builder::new()
                .suffix(".toml")
                .tempfile()
                .expect("temp config");
            writeln!(
                file,
                "[tei]\nmax-retries = 4\n[workers]\ningest-lanes = 6\n[search]\nhnsw-ef = 300"
            )
            .expect("write config");
            unsafe {
                env::set_var("AXON_CONFIG_PATH", file.path());
                env::remove_var("TEI_MAX_RETRIES");
                env::remove_var("AXON_INGEST_LANES");
                env::remove_var("AXON_HNSW_EF_SEARCH");
            }

            let cfg = Config::default_minimal();

            assert_eq!(cfg.tei_max_retries, 4);
            assert_eq!(cfg.ingest_lanes, 6);
            assert_eq!(cfg.hnsw_ef_search, 300);
        },
    );
}

#[test]
fn config_default_output_flags() {
    let cfg = Config::default();
    assert!(!cfg.wait);
    assert!(!cfg.json_output);
    assert_eq!(cfg.evaluate_responses_mode, EvaluateResponsesMode::Inline);
    assert_eq!(cfg.mcp_transport, McpTransport::Stdio);
    assert_eq!(cfg.mcp_http_host, "127.0.0.1");
    assert_eq!(cfg.mcp_http_port, 8001);
    assert!(!cfg.reclaimed_status_only);
    assert!(!cfg.active_status_only);
    assert!(!cfg.recent_status_only);
}

#[test]
fn config_default_secrets_are_empty() {
    let cfg = Config::default();
    assert!(cfg.tavily_api_key.is_empty());
    assert!(cfg.github_token.is_none());
    assert!(cfg.reddit_client_id.is_none());
    assert!(cfg.reddit_client_secret.is_none());
}

#[test]
fn config_default_sessions_flags_off() {
    let cfg = Config::default();
    assert!(!cfg.sessions_claude);
    assert!(!cfg.sessions_codex);
    assert!(!cfg.sessions_gemini);
    assert!(cfg.sessions_project.is_none());
}

#[test]
fn config_debug_redacts_secrets() {
    let cfg = Config {
        tavily_api_key: "tvly-supersecret".to_string(),
        github_token: Some("ghp_supersecret".to_string()),
        reddit_client_id: Some("my-reddit-id".to_string()),
        reddit_client_secret: Some("my-reddit-secret".to_string()),
        ..Config::default()
    };

    let debug_output = format!("{cfg:?}");

    // Secrets must NOT appear in Debug output.
    assert!(
        !debug_output.contains("supersecret"),
        "tavily_api_key or github_token leaked"
    );
    assert!(
        !debug_output.contains("my-reddit-id"),
        "reddit_client_id leaked"
    );
    assert!(
        !debug_output.contains("my-reddit-secret"),
        "reddit_client_secret leaked"
    );

    // Redaction markers must be present.
    assert!(
        debug_output.contains("[REDACTED]"),
        "no [REDACTED] marker found"
    );
}

#[test]
fn test_config_debug_includes_sessions_fields() {
    let cfg = Config {
        sessions_claude: true,
        sessions_codex: false,
        sessions_gemini: true,
        ..Config::default()
    };

    let debug_output = format!("{cfg:?}");
    assert!(debug_output.contains("sessions_claude: true"));
    assert!(debug_output.contains("sessions_codex: false"));
    assert!(debug_output.contains("sessions_gemini: true"));
}

// --- Performance profile range tests ---

/// Replicates the computation from `src/core/config/parse/performance.rs`
/// so we can test all four profiles without depending on the private module.
/// Returns (crawl_concurrency, backfill_concurrency, timeout_ms, retries, backoff_ms).
fn profile_defaults(profile: PerformanceProfile) -> (usize, usize, u64, usize, u64) {
    let logical_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8);
    match profile {
        PerformanceProfile::HighStable => (
            (logical_cpus.saturating_mul(8)).clamp(64, 192),
            (logical_cpus.saturating_mul(6)).clamp(32, 128),
            20_000,
            2,
            250,
        ),
        PerformanceProfile::Extreme => (
            (logical_cpus.saturating_mul(16)).clamp(128, 384),
            (logical_cpus.saturating_mul(10)).clamp(64, 256),
            15_000,
            1,
            100,
        ),
        PerformanceProfile::Balanced => (
            (logical_cpus.saturating_mul(4)).clamp(32, 96),
            (logical_cpus.saturating_mul(3)).clamp(16, 64),
            30_000,
            2,
            300,
        ),
        PerformanceProfile::Max => (
            (logical_cpus.saturating_mul(24)).clamp(256, 1024),
            (logical_cpus.saturating_mul(20)).clamp(128, 1024),
            12_000,
            1,
            50,
        ),
    }
}

#[test]
fn test_high_stable_profile_within_bounds() {
    let (crawl, backfill, timeout, retries, backoff) =
        profile_defaults(PerformanceProfile::HighStable);
    assert!((64..=192).contains(&crawl), "crawl={crawl} out of [64,192]");
    assert!(
        (32..=128).contains(&backfill),
        "backfill={backfill} out of [32,128]"
    );
    assert_eq!(timeout, 20_000, "timeout should be 20s");
    assert_eq!(retries, 2);
    assert_eq!(backoff, 250);
}

#[test]
fn test_extreme_profile_within_bounds() {
    let (crawl, backfill, timeout, retries, backoff) =
        profile_defaults(PerformanceProfile::Extreme);
    assert!(
        (128..=384).contains(&crawl),
        "crawl={crawl} out of [128,384]"
    );
    assert!(
        (64..=256).contains(&backfill),
        "backfill={backfill} out of [64,256]"
    );
    assert_eq!(timeout, 15_000, "timeout should be 15s");
    assert_eq!(retries, 1);
    assert_eq!(backoff, 100);
}

#[test]
fn test_balanced_profile_within_bounds() {
    let (crawl, backfill, timeout, retries, backoff) =
        profile_defaults(PerformanceProfile::Balanced);
    assert!((32..=96).contains(&crawl), "crawl={crawl} out of [32,96]");
    assert!(
        (16..=64).contains(&backfill),
        "backfill={backfill} out of [16,64]"
    );
    assert_eq!(timeout, 30_000, "timeout should be 30s");
    assert_eq!(retries, 2);
    assert_eq!(backoff, 300);
}

#[test]
fn test_max_profile_within_bounds() {
    let (crawl, backfill, timeout, retries, backoff) = profile_defaults(PerformanceProfile::Max);
    assert!(
        (256..=1024).contains(&crawl),
        "crawl={crawl} out of [256,1024]"
    );
    assert!(
        (128..=1024).contains(&backfill),
        "backfill={backfill} out of [128,1024]"
    );
    assert_eq!(timeout, 12_000, "timeout should be 12s");
    assert_eq!(retries, 1);
    assert_eq!(backoff, 50);
}

#[test]
fn test_extreme_crawl_concurrency_exceeds_balanced() {
    let (extreme_crawl, ..) = profile_defaults(PerformanceProfile::Extreme);
    let (balanced_crawl, ..) = profile_defaults(PerformanceProfile::Balanced);
    assert!(
        extreme_crawl > balanced_crawl,
        "extreme crawl concurrency ({extreme_crawl}) should exceed balanced ({balanced_crawl})"
    );
}

#[test]
fn test_max_crawl_concurrency_exceeds_extreme() {
    let (max_crawl, ..) = profile_defaults(PerformanceProfile::Max);
    let (extreme_crawl, ..) = profile_defaults(PerformanceProfile::Extreme);
    assert!(
        max_crawl > extreme_crawl,
        "max crawl concurrency ({max_crawl}) should exceed extreme ({extreme_crawl})"
    );
}

#[test]
fn new_engine_tuning_defaults() {
    let cfg = Config::default();
    assert_eq!(cfg.chrome_network_idle_timeout_secs, 15);
    assert!((cfg.auto_switch_thin_ratio - 0.60).abs() < f64::EPSILON);
    assert_eq!(cfg.auto_switch_min_pages, 10);
    assert_eq!(cfg.crawl_broadcast_buffer_min, 512);
    assert_eq!(cfg.crawl_broadcast_buffer_max, 2_048);
    assert!(!cfg.allow_unbounded_broad_crawl);
    assert_eq!(cfg.crawl_memory_abort_percent, Some(85.0));
}

#[test]
fn new_spider_builder_defaults() {
    let cfg = Config::default();
    assert!(cfg.url_whitelist.is_empty());
    assert!(!cfg.block_assets);
    assert_eq!(cfg.max_page_bytes, Some(4 * 1024 * 1024));
    assert!(!cfg.redirect_policy_strict);
    assert!(cfg.chrome_wait_for_selector.is_none());
    assert!(!cfg.chrome_screenshot);
}

#[test]
fn new_spider_agent_defaults() {
    let cfg = Config::default();
    assert!(cfg.research_depth.is_none());
    assert!(cfg.search_time_range.is_none());
}

#[test]
fn default_config_has_github_issue_pr_limits() {
    let cfg = Config::default();
    assert_eq!(cfg.github_max_issues, 100);
    assert_eq!(cfg.github_max_prs, 100);
}
