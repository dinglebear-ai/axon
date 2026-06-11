use super::build_config::tests::ENV_LOCK;
use super::docker::is_docker_service_host;
use crate::core::config::types::{CommandKind, McpTransport};
use clap::Parser;
use std::env;

#[allow(unsafe_code)]
#[test]
fn parse_watch_create_with_every_and_type() {
    let _guard = ENV_LOCK.lock().unwrap();

    let cli = super::Cli::parse_from([
        "axon",
        "--tei-url",
        "http://127.0.0.1:52000",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "watch",
        "create",
        "docs-watch",
        "--task-type",
        "watch",
        "--every-seconds",
        "300",
    ]);
    let cfg = super::build_config::into_config(cli).expect("watch create should parse");
    assert!(matches!(cfg.command, CommandKind::Watch));
    assert_eq!(
        cfg.positional,
        vec![
            "create".to_string(),
            "docs-watch".to_string(),
            "--task-type".to_string(),
            "watch".to_string(),
            "--every-seconds".to_string(),
            "300".to_string(),
        ]
    );
}

#[test]
fn config_example_toml_parses() {
    let contents = include_str!("../../../config.example.toml");
    let cfg = super::toml_config::load_toml_config_from_str(contents)
        .expect("config.example.toml should parse");
    assert!(
        cfg.search.ask_hybrid_candidates.is_none(),
        "example documents defaults without forcing local overrides"
    );
    assert!(
        cfg.tei.max_retries.is_none(),
        "example documents defaults without forcing local overrides"
    );
}

#[allow(unsafe_code)]
#[test]
fn parse_watch_run_now() {
    let _guard = ENV_LOCK.lock().unwrap();

    let cli = super::Cli::parse_from([
        "axon",
        "--tei-url",
        "http://127.0.0.1:52000",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "watch",
        "run-now",
        "11111111-1111-4111-8111-111111111111",
    ]);
    let cfg = super::build_config::into_config(cli).expect("watch run-now should parse");
    assert!(matches!(cfg.command, CommandKind::Watch));
    assert_eq!(
        cfg.positional,
        vec![
            "run-now".to_string(),
            "11111111-1111-4111-8111-111111111111".to_string(),
        ]
    );
}

#[allow(unsafe_code)]
#[test]
fn parse_watch_history_with_limit() {
    let _guard = ENV_LOCK.lock().unwrap();

    let cli = super::Cli::parse_from([
        "axon",
        "--tei-url",
        "http://127.0.0.1:52000",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "watch",
        "history",
        "11111111-1111-4111-8111-111111111111",
        "--limit",
        "25",
    ]);
    let cfg = super::build_config::into_config(cli).expect("watch history should parse");
    assert!(matches!(cfg.command, CommandKind::Watch));
    assert_eq!(
        cfg.positional,
        vec![
            "history".to_string(),
            "11111111-1111-4111-8111-111111111111".to_string(),
            "--limit".to_string(),
            "25".to_string(),
        ]
    );
}

#[allow(unsafe_code)]
#[test]
fn parse_completions_bash_does_not_require_service_envs() {
    let _guard = ENV_LOCK.lock().unwrap();
    let cli = super::Cli::parse_from(["axon", "completions", "bash"]);
    let cfg = super::build_config::into_config(cli)
        .expect("completions should parse without service env vars");
    assert!(matches!(cfg.command, CommandKind::Completions));
    assert_eq!(cfg.positional, vec!["bash".to_string()]);
}

#[test]
fn parse_completion_alias_is_accepted() {
    let result = super::Cli::try_parse_from(["axon", "completion", "zsh"]);
    assert!(result.is_ok(), "completion alias should be accepted");
}

#[allow(unsafe_code)]
#[test]
fn parse_sources_domain_flags_into_config() {
    let _guard = ENV_LOCK.lock().unwrap();

    let cli = super::Cli::parse_from([
        "axon",
        "--tei-url",
        "http://127.0.0.1:52000",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "sources",
        "--domain",
        "Docs.RS",
        "--all",
    ]);
    let cfg = super::build_config::into_config(cli).expect("sources domain should parse");
    assert!(matches!(cfg.command, CommandKind::Sources));
    assert_eq!(cfg.sources_domain.as_deref(), Some("Docs.RS"));
    assert!(cfg.sources_domain_all);
    assert!(cfg.domains_domain.is_none());
}

#[allow(unsafe_code)]
#[test]
fn parse_domains_domain_flag_into_config() {
    let _guard = ENV_LOCK.lock().unwrap();

    let cli = super::Cli::parse_from([
        "axon",
        "--tei-url",
        "http://127.0.0.1:52000",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "domains",
        "--domain",
        "docs.rs",
    ]);
    let cfg = super::build_config::into_config(cli).expect("domains domain should parse");
    assert!(matches!(cfg.command, CommandKind::Domains));
    assert_eq!(cfg.domains_domain.as_deref(), Some("docs.rs"));
    assert!(cfg.sources_domain.is_none());
    assert!(!cfg.sources_domain_all);
}

#[test]
fn parse_retrieve_max_points_into_config() {
    let cli = super::Cli::parse_from([
        "axon",
        "--tei-url",
        "http://127.0.0.1:52000",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "retrieve",
        "https://example.com/docs",
        "--max-points",
        "25",
    ]);
    let cfg = super::build_config::into_config(cli).expect("retrieve should parse");
    assert!(matches!(cfg.command, CommandKind::Retrieve));
    assert_eq!(cfg.positional, vec!["https://example.com/docs".to_string()]);
    assert_eq!(cfg.retrieve_max_points, Some(25));
}

// --- ask session-management flag tests ---

fn ask_cli(extra: &[&str]) -> super::Cli {
    let mut argv: Vec<&str> = vec![
        "axon",
        "--tei-url",
        "http://127.0.0.1:52000",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "ask",
    ];
    argv.extend_from_slice(extra);
    super::Cli::parse_from(argv)
}

fn try_ask_cli(extra: &[&str]) -> Result<super::Cli, clap::Error> {
    let mut argv: Vec<&str> = vec![
        "axon",
        "--tei-url",
        "http://127.0.0.1:52000",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "ask",
    ];
    argv.extend_from_slice(extra);
    super::Cli::try_parse_from(argv)
}

#[test]
fn parse_ask_continue_alias_is_equivalent_to_follow_up() {
    let cfg_a =
        super::build_config::into_config(ask_cli(&["--follow-up", "q"])).expect("follow-up");
    let cfg_b = super::build_config::into_config(ask_cli(&["--continue", "q"])).expect("continue");
    let cfg_c = super::build_config::into_config(ask_cli(&["-c", "q"])).expect("-c");
    assert!(cfg_a.ask_follow_up);
    assert!(cfg_b.ask_follow_up);
    assert!(cfg_c.ask_follow_up);
}

#[test]
fn parse_ask_resume_is_alias_for_follow_up_plus_session() {
    let cfg = super::build_config::into_config(ask_cli(&["--resume", "rust-thread", "q"]))
        .expect("resume");
    assert!(cfg.ask_follow_up);
    assert_eq!(cfg.ask_session.as_deref(), Some("rust-thread"));
}

#[test]
fn parse_ask_new_session_conflicts_with_follow_up() {
    let result = try_ask_cli(&["--new-session", "--follow-up", "q"]);
    assert!(result.is_err(), "--new-session + --follow-up should error");
}

#[test]
fn parse_ask_resume_conflicts_with_session() {
    let result = try_ask_cli(&["--resume", "a", "--session", "b", "q"]);
    assert!(result.is_err(), "--resume + --session should error");
}

#[test]
fn parse_ask_reset_session_conflicts_with_new_session() {
    let result = try_ask_cli(&["--new-session", "--reset-session", "q"]);
    assert!(
        result.is_err(),
        "--new-session + --reset-session should error"
    );
}

#[test]
fn parse_ask_list_sessions_with_query_rejected() {
    let err = super::build_config::into_config(ask_cli(&["--list-sessions", "stray-query"]))
        .expect_err("list-sessions + query must fail");
    assert!(err.contains("--list-sessions"));
}

#[test]
fn parse_ask_list_sessions_with_query_flag_rejected() {
    let err = super::build_config::into_config(ask_cli(&[
        "--query",
        "stray-via-flag",
        "--list-sessions",
    ]))
    .expect_err("list-sessions + --query must fail");
    assert!(err.contains("--list-sessions"));
}

#[test]
fn parse_ask_resume_conflicts_with_new_session() {
    let result = try_ask_cli(&["--resume", "rust-thread", "--new-session", "q"]);
    assert!(result.is_err(), "--resume + --new-session should error");
}

#[test]
fn parse_ask_resume_conflicts_with_reset_session() {
    let result = try_ask_cli(&["--resume", "rust-thread", "--reset-session", "q"]);
    assert!(result.is_err(), "--resume + --reset-session should error");
}

#[test]
fn parse_ask_list_sessions_alone_is_ok() {
    let cfg =
        super::build_config::into_config(ask_cli(&["--list-sessions"])).expect("list-sessions");
    assert!(matches!(cfg.command, CommandKind::Ask));
    assert!(cfg.ask_list_sessions);
}

#[test]
fn parse_ask_new_session_records_flag() {
    let cfg = super::build_config::into_config(ask_cli(&["--new-session", "--session", "x", "q"]))
        .expect("new-session");
    assert!(cfg.ask_new_session);
    assert_eq!(cfg.ask_session.as_deref(), Some("x"));
}

// --- is_docker_service_host tests ---

#[test]
fn test_is_docker_service_host_recognizes_all_known_services() {
    assert!(is_docker_service_host("axon-qdrant"));
    assert!(is_docker_service_host("axon-tei"));
    assert!(is_docker_service_host("axon-ollama"));
    assert!(is_docker_service_host("axon-chrome"));
}

#[test]
fn test_is_docker_service_host_rejects_unknown_hyphenated_hosts() {
    // These look like Docker-style names but are NOT in HOST_MAP.
    assert!(!is_docker_service_host("my-home-server"));
    assert!(!is_docker_service_host("custom-chrome-host"));
    assert!(!is_docker_service_host("prod-infra"));
    assert!(!is_docker_service_host("axon-unknown"));
}

#[test]
fn test_is_docker_service_host_rejects_plain_hosts() {
    assert!(!is_docker_service_host("localhost"));
    assert!(!is_docker_service_host("127.0.0.1"));
    assert!(!is_docker_service_host("example.com"));
    assert!(!is_docker_service_host(""));
}

#[allow(unsafe_code)]
#[test]
fn test_tavily_api_key_read_from_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    const VAR: &str = "AXON_TEST_TAVILY_KEY_PRESENT";
    // SAFETY: guarded by ENV_LOCK; no other test mutates this var concurrently.
    unsafe { env::set_var(VAR, "test-key-123") };
    let key = env::var(VAR).ok().unwrap_or_default();
    assert_eq!(key, "test-key-123");
    unsafe { env::remove_var(VAR) };
}

#[test]
fn test_tavily_api_key_defaults_to_empty_when_unset() {
    let _guard = ENV_LOCK.lock().unwrap();
    const VAR: &str = "AXON_TEST_TAVILY_KEY_ABSENT";
    // This var is never set anywhere, so it should always be absent.
    let key = env::var(VAR).ok().unwrap_or_default();
    assert_eq!(key, "");
}

// --- exclude prefix disable-by-empty tests ---

#[test]
fn test_empty_string_disables_default_exclude_prefixes() {
    // Passing "" should set `disable_defaults = true`, suppressing the
    // built-in locale-prefix exclusions without adding any custom prefixes.
    let normalized = super::excludes::normalize_exclude_prefixes(vec!["".to_string()]);
    assert!(
        normalized.disable_defaults,
        "empty string should set disable_defaults = true"
    );
    assert!(
        normalized.prefixes.is_empty(),
        "empty string should not produce any prefix entries"
    );
}

// --- parse_viewport tests ---

#[test]
fn test_parse_viewport_standard() {
    assert_eq!(super::helpers::parse_viewport("1920x1080"), (1920, 1080));
}

#[test]
fn test_parse_viewport_small() {
    assert_eq!(super::helpers::parse_viewport("800x600"), (800, 600));
}

#[test]
fn test_parse_viewport_bad_input_falls_back() {
    assert_eq!(super::helpers::parse_viewport("bad"), (1920, 1080));
}

#[test]
fn test_parse_viewport_missing_height_falls_back() {
    assert_eq!(super::helpers::parse_viewport("1920x"), (1920, 1080));
}

#[test]
fn test_parse_viewport_zero_dimension_falls_back() {
    assert_eq!(super::helpers::parse_viewport("0x1080"), (1920, 1080));
}

#[test]
fn test_parse_viewport_zero_both_dimensions_falls_back() {
    // Both width and height are 0 — guard is `w > 0 && h > 0`
    assert_eq!(super::helpers::parse_viewport("0x0"), (1920, 1080));
}

#[test]
fn test_parse_viewport_empty_string_falls_back() {
    assert_eq!(super::helpers::parse_viewport(""), (1920, 1080));
}

#[test]
fn test_parse_viewport_uppercase_x_falls_back() {
    // split_once is case-sensitive; 'X' != 'x', so no separator is found
    assert_eq!(super::helpers::parse_viewport("1920X1080"), (1920, 1080));
}

#[test]
fn test_parse_viewport_surrounding_spaces_trimmed() {
    // The code calls .trim() on each component before parsing
    assert_eq!(super::helpers::parse_viewport(" 1280 x 720 "), (1280, 720));
}

#[test]
fn test_parse_viewport_large_positive_values_accepted() {
    assert_eq!(
        super::helpers::parse_viewport("99999x99999"),
        (99999, 99999)
    );
}

// --- normalize_local_service_url tests ---

#[test]
fn test_normalize_url_unrecognized_hostname_unchanged() {
    let input = "postgresql://user:pass@some-other-host:5432/db".to_string();
    assert_eq!(
        super::docker::normalize_local_service_url(input.clone()),
        input
    );
}

#[test]
fn test_normalize_url_non_url_string_unchanged() {
    let input = "not-a-url-at-all".to_string();
    assert_eq!(
        super::docker::normalize_local_service_url(input.clone()),
        input
    );
}

#[test]
fn test_normalize_url_empty_string_unchanged() {
    let input = String::new();
    assert_eq!(
        super::docker::normalize_local_service_url(input.clone()),
        input
    );
}

#[test]
fn test_normalize_url_qdrant_rewrites_when_not_in_docker() {
    if std::path::Path::new("/.dockerenv").exists() {
        return;
    }
    use spider::url::Url;
    let url = "http://axon-qdrant:6333/collections".to_string();
    let result = super::docker::normalize_local_service_url(url);
    let parsed = Url::parse(&result).unwrap();
    assert_eq!(parsed.host_str(), Some("127.0.0.1"));
    assert_eq!(parsed.port(), Some(53333));
}

#[test]
fn test_normalize_url_ollama_rewrites_when_not_in_docker() {
    if std::path::Path::new("/.dockerenv").exists() {
        return;
    }
    use spider::url::Url;
    let url = "http://axon-ollama:11434/api/generate".to_string();
    let result = super::docker::normalize_local_service_url(url);
    let parsed = Url::parse(&result).unwrap();
    assert_eq!(parsed.host_str(), Some("127.0.0.1"));
    assert_eq!(parsed.port(), Some(11434));
}

#[test]
fn test_normalize_url_credentials_preserved_after_rewrite() {
    if std::path::Path::new("/.dockerenv").exists() {
        return;
    }
    use spider::url::Url;
    let url = "http://user:pass@axon-qdrant:6333/collections".to_string();
    let result = super::docker::normalize_local_service_url(url);
    let parsed = Url::parse(&result).unwrap();
    assert_eq!(parsed.host_str(), Some("127.0.0.1"));
    assert_eq!(parsed.port(), Some(53333));
    assert_eq!(parsed.username(), "user");
    assert_eq!(parsed.password(), Some("pass"));
    assert_eq!(parsed.path(), "/collections");
}

#[test]
fn test_slash_disables_default_exclude_prefixes() {
    // "/" is treated identically to "" — it disables default exclusions.
    let normalized = super::excludes::normalize_exclude_prefixes(vec!["/".to_string()]);
    assert!(
        normalized.disable_defaults,
        "bare slash should set disable_defaults = true"
    );
    assert!(
        normalized.prefixes.is_empty(),
        "bare slash should not produce any prefix entries"
    );
}

#[allow(unsafe_code)]
#[test]
fn parse_mcp_defaults_to_stdio_transport() {
    let _guard = ENV_LOCK.lock().unwrap();

    let cli = super::Cli::parse_from([
        "axon",
        "--tei-url",
        "http://127.0.0.1:52000",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "mcp",
    ]);
    let cfg = super::build_config::into_config(cli).expect("mcp config should parse");
    assert!(matches!(cfg.command, CommandKind::Mcp));
    assert_eq!(cfg.mcp_transport, McpTransport::Stdio);
}

#[allow(unsafe_code)]
#[test]
fn parse_mcp_transport_flag_overrides_command_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    const TRANSPORT: &str = "AXON_MCP_TRANSPORT";

    unsafe {
        env::set_var(TRANSPORT, "stdio");
    }

    let cli = super::Cli::parse_from([
        "axon",
        "--tei-url",
        "http://127.0.0.1:52000",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "mcp",
        "--transport",
        "both",
    ]);
    let cfg = super::build_config::into_config(cli).expect("mcp config should parse");
    assert_eq!(cfg.mcp_transport, McpTransport::Both);

    unsafe {
        env::remove_var(TRANSPORT);
    }
}

#[allow(unsafe_code)]
#[test]
fn parse_serve_mcp_maps_to_mcp_http_transport() {
    let _guard = ENV_LOCK.lock().unwrap();
    let cli = super::Cli::parse_from([
        "axon",
        "--tei-url",
        "http://127.0.0.1:52000",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "serve",
        "mcp",
    ]);
    let cfg = super::build_config::into_config(cli).expect("serve mcp config should parse");
    assert!(matches!(cfg.command, CommandKind::Mcp));
    assert_eq!(cfg.mcp_transport, McpTransport::Http);
}

#[allow(unsafe_code)]
#[test]
fn parse_mcp_transport_env_overrides_command_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    const TRANSPORT: &str = "AXON_MCP_TRANSPORT";
    unsafe {
        env::set_var(TRANSPORT, "both");
    }
    let cli = super::Cli::parse_from([
        "axon",
        "--tei-url",
        "http://127.0.0.1:52000",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "mcp",
    ]);
    let cfg = super::build_config::into_config(cli).expect("mcp config should parse");
    unsafe { env::remove_var(TRANSPORT) };
    assert_eq!(cfg.mcp_transport, McpTransport::Both);
}

#[allow(unsafe_code)]
#[test]
fn parse_setup_targets_does_not_require_service_urls() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        env::remove_var("TEI_URL");
        env::remove_var("QDRANT_URL");
    }

    let cli = super::Cli::parse_from(["axon", "--json", "setup", "targets"]);
    let cfg = super::build_config::into_config(cli).expect("setup targets should parse");

    assert!(matches!(cfg.command, CommandKind::Setup));
    assert_eq!(cfg.positional, vec!["targets".to_string()]);
    assert!(cfg.json_output);
}

#[allow(unsafe_code)]
#[test]
fn parse_setup_without_subcommand_is_local_first_run() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        env::remove_var("TEI_URL");
        env::remove_var("QDRANT_URL");
    }

    let cli = super::Cli::parse_from(["axon", "setup"]);
    let cfg = super::build_config::into_config(cli).expect("setup should parse");

    assert!(matches!(cfg.command, CommandKind::Setup));
    assert!(cfg.positional.is_empty());
}

#[allow(unsafe_code)]
#[test]
fn parse_setup_init_preflight_smoke_and_stack_modes() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        env::remove_var("TEI_URL");
        env::remove_var("QDRANT_URL");
    }

    let hook = super::Cli::parse_from(["axon", "setup", "plugin-hook"]);
    let hook_cfg = super::build_config::into_config(hook).expect("setup plugin-hook should parse");
    assert_eq!(hook_cfg.positional, vec!["plugin-hook".to_string()]);

    let hook_no_setup = super::Cli::parse_from(["axon", "setup", "plugin-hook", "--no-setup"]);
    let hook_no_setup_cfg = super::build_config::into_config(hook_no_setup)
        .expect("setup plugin-hook --no-setup should parse");
    assert_eq!(
        hook_no_setup_cfg.positional,
        vec!["plugin-hook".to_string(), "--no-setup".to_string()]
    );

    let removed_hook_flag =
        super::Cli::try_parse_from(["axon", "setup", "plugin-hook", concat!("--no-", "repair")]);
    assert!(
        removed_hook_flag.is_err(),
        "removed plugin hook flag should be rejected"
    );

    let init = super::Cli::parse_from([
        "axon",
        "setup",
        "init",
        "--auth-mode",
        "oauth",
        "--mcp-host",
        "0.0.0.0",
        "--mcp-port",
        "9000",
    ]);
    let init_cfg = super::build_config::into_config(init).expect("setup init should parse");
    assert_eq!(
        init_cfg.positional,
        vec![
            "init".to_string(),
            "--mcp-host".to_string(),
            "0.0.0.0".to_string(),
            "--mcp-port".to_string(),
            "9000".to_string(),
            "--auth-mode".to_string(),
            "oauth".to_string()
        ]
    );

    let preflight = super::Cli::parse_from(["axon", "preflight"]);
    let preflight_cfg =
        super::build_config::into_config(preflight).expect("preflight should parse");
    assert!(matches!(preflight_cfg.command, CommandKind::Preflight));

    let smoke = super::Cli::parse_from(["axon", "smoke"]);
    let smoke_cfg = super::build_config::into_config(smoke).expect("smoke should parse");
    assert!(matches!(smoke_cfg.command, CommandKind::Smoke));

    let compose = super::Cli::parse_from(["axon", "compose", "restart"]);
    let compose_cfg =
        super::build_config::into_config(compose).expect("compose restart should parse");
    assert!(matches!(compose_cfg.command, CommandKind::Compose));
    assert_eq!(compose_cfg.positional, vec!["restart".to_string()]);

    let removed_flag = concat!("--", "migrate", "-env");
    let repair_with_removed_flag =
        super::Cli::try_parse_from(["axon", "setup", "repair", removed_flag]);
    assert!(
        repair_with_removed_flag.is_err(),
        "removed setup command should be rejected"
    );
}

#[test]
fn parse_sessions_watch_provider_and_project_filters_are_typed() {
    let cli = super::Cli::parse_from([
        "axon",
        "--tei-url",
        "http://127.0.0.1:52000",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "sessions",
        "watch",
        "--codex",
        "--project",
        "axon",
        "--no-initial-scan",
    ]);
    let cfg = super::build_config::into_config(cli).expect("sessions watch should parse");

    assert!(cfg.sessions_watch.is_some());
    assert!(cfg.sessions_codex);
    assert!(!cfg.sessions_claude);
    assert!(!cfg.sessions_gemini);
    assert_eq!(cfg.sessions_project.as_deref(), Some("axon"));
    assert!(cfg.positional.is_empty());
}

#[allow(unsafe_code)]
#[test]
fn parse_setup_session_watch_service_actions_are_typed() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        env::remove_var("TEI_URL");
        env::remove_var("QDRANT_URL");
    }

    use crate::core::config::SessionWatchServiceAction;
    for (subcommand, expected) in [
        ("install", SessionWatchServiceAction::Install),
        ("check", SessionWatchServiceAction::Check),
        ("remove", SessionWatchServiceAction::Remove),
        ("status", SessionWatchServiceAction::Status),
    ] {
        let cli = super::Cli::parse_from(["axon", "setup", "session-watch-service", subcommand]);
        let cfg = super::build_config::into_config(cli)
            .expect("setup session-watch-service should parse");

        assert!(matches!(cfg.command, CommandKind::Setup));
        assert_eq!(cfg.setup_session_watch_action, Some(expected));
        assert!(cfg.positional.is_empty());
    }
}
