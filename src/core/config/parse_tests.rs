use super::build_config::tests::ENV_LOCK;
use super::docker::is_docker_service_host;
use crate::core::config::types::{CommandKind, McpTransport};
use clap::Parser;
use std::env;

#[allow(unsafe_code)]
#[test]
fn parse_watch_create_with_every_and_type() {
    let _guard = ENV_LOCK.lock().unwrap();
    const PG: &str = "AXON_PG_URL";
    const REDIS: &str = "AXON_REDIS_URL";
    const AMQP: &str = "AXON_AMQP_URL";
    unsafe {
        env::set_var(PG, "postgresql://axon:postgres@*********:53432/axon");
        env::set_var(REDIS, "redis://*********:53379");
        env::set_var(AMQP, "amqp://axon:axonrabbit@*********:45535/%2f");
    }

    let cli = super::Cli::parse_from([
        "axon",
        "--tei-url",
        "http://127.0.0.1:52000",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "watch",
        "create",
        "docs-refresh",
        "--task-type",
        "refresh",
        "--every-seconds",
        "300",
    ]);
    let cfg = super::build_config::into_config(cli).expect("watch create should parse");
    assert!(matches!(cfg.command, CommandKind::Watch));
    assert_eq!(
        cfg.positional,
        vec![
            "create".to_string(),
            "docs-refresh".to_string(),
            "--task-type".to_string(),
            "refresh".to_string(),
            "--every-seconds".to_string(),
            "300".to_string(),
        ]
    );

    unsafe {
        env::remove_var(PG);
        env::remove_var(REDIS);
        env::remove_var(AMQP);
    }
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
    const PG: &str = "AXON_PG_URL";
    const REDIS: &str = "AXON_REDIS_URL";
    const AMQP: &str = "AXON_AMQP_URL";
    unsafe {
        env::set_var(PG, "postgresql://axon:postgres@*********:53432/axon");
        env::set_var(REDIS, "redis://*********:53379");
        env::set_var(AMQP, "amqp://axon:axonrabbit@*********:45535/%2f");
    }

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

    unsafe {
        env::remove_var(PG);
        env::remove_var(REDIS);
        env::remove_var(AMQP);
    }
}

#[allow(unsafe_code)]
#[test]
fn parse_watch_history_with_limit() {
    let _guard = ENV_LOCK.lock().unwrap();
    const PG: &str = "AXON_PG_URL";
    const REDIS: &str = "AXON_REDIS_URL";
    const AMQP: &str = "AXON_AMQP_URL";
    unsafe {
        env::set_var(PG, "postgresql://axon:postgres@*********:53432/axon");
        env::set_var(REDIS, "redis://*********:53379");
        env::set_var(AMQP, "amqp://axon:axonrabbit@*********:45535/%2f");
    }

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

    unsafe {
        env::remove_var(PG);
        env::remove_var(REDIS);
        env::remove_var(AMQP);
    }
}

#[allow(unsafe_code)]
#[test]
fn parse_completions_bash_does_not_require_service_envs() {
    let _guard = ENV_LOCK.lock().unwrap();
    const PG: &str = "AXON_PG_URL";
    const REDIS: &str = "AXON_REDIS_URL";
    const AMQP: &str = "AXON_AMQP_URL";
    let prev_pg = env::var(PG).ok();
    let prev_redis = env::var(REDIS).ok();
    let prev_amqp = env::var(AMQP).ok();

    unsafe {
        env::remove_var(PG);
        env::remove_var(REDIS);
        env::remove_var(AMQP);
    }

    let cli = super::Cli::parse_from(["axon", "completions", "bash"]);
    let cfg = super::build_config::into_config(cli)
        .expect("completions should parse without service env vars");
    assert!(matches!(cfg.command, CommandKind::Completions));
    assert_eq!(cfg.positional, vec!["bash".to_string()]);

    match prev_pg {
        Some(v) => unsafe { env::set_var(PG, v) },
        None => unsafe { env::remove_var(PG) },
    }
    match prev_redis {
        Some(v) => unsafe { env::set_var(REDIS, v) },
        None => unsafe { env::remove_var(REDIS) },
    }
    match prev_amqp {
        Some(v) => unsafe { env::set_var(AMQP, v) },
        None => unsafe { env::remove_var(AMQP) },
    }
}

#[test]
fn parse_completion_alias_is_accepted() {
    let result = super::Cli::try_parse_from(["axon", "completion", "zsh"]);
    assert!(result.is_ok(), "completion alias should be accepted");
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
    const PG: &str = "AXON_PG_URL";
    const REDIS: &str = "AXON_REDIS_URL";
    const AMQP: &str = "AXON_AMQP_URL";

    unsafe {
        env::set_var(PG, "postgresql://axon:postgres@127.0.0.1:53432/axon");
        env::set_var(REDIS, "redis://127.0.0.1:53379");
        env::set_var(AMQP, "amqp://axon:axonrabbit@127.0.0.1:45535/%2f");
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
    assert!(matches!(cfg.command, CommandKind::Mcp));
    assert_eq!(cfg.mcp_transport, McpTransport::Stdio);

    unsafe {
        env::remove_var(PG);
        env::remove_var(REDIS);
        env::remove_var(AMQP);
    }
}

#[allow(unsafe_code)]
#[test]
fn parse_mcp_transport_flag_overrides_command_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    const PG: &str = "AXON_PG_URL";
    const REDIS: &str = "AXON_REDIS_URL";
    const AMQP: &str = "AXON_AMQP_URL";
    const TRANSPORT: &str = "AXON_MCP_TRANSPORT";

    unsafe {
        env::set_var(PG, "postgresql://axon:postgres@127.0.0.1:53432/axon");
        env::set_var(REDIS, "redis://127.0.0.1:53379");
        env::set_var(AMQP, "amqp://axon:axonrabbit@127.0.0.1:45535/%2f");
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
        env::remove_var(PG);
        env::remove_var(REDIS);
        env::remove_var(AMQP);
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
fn parse_setup_check_and_repair_modes() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        env::remove_var("TEI_URL");
        env::remove_var("QDRANT_URL");
    }

    let hook = super::Cli::parse_from(["axon", "setup", "plugin-hook"]);
    let hook_cfg = super::build_config::into_config(hook).expect("setup plugin-hook should parse");
    assert_eq!(hook_cfg.positional, vec!["plugin-hook".to_string()]);

    let hook_no_repair = super::Cli::parse_from(["axon", "setup", "plugin-hook", "--no-repair"]);
    let hook_no_repair_cfg = super::build_config::into_config(hook_no_repair)
        .expect("setup plugin-hook --no-repair should parse");
    assert_eq!(
        hook_no_repair_cfg.positional,
        vec!["plugin-hook".to_string(), "--no-repair".to_string()]
    );

    let check = super::Cli::parse_from(["axon", "setup", "check"]);
    let check_cfg = super::build_config::into_config(check).expect("setup check should parse");
    assert_eq!(check_cfg.positional, vec!["check".to_string()]);

    let repair = super::Cli::parse_from(["axon", "setup", "repair"]);
    let repair_cfg = super::build_config::into_config(repair).expect("setup repair should parse");
    assert_eq!(repair_cfg.positional, vec!["repair".to_string()]);

    let migrate = super::Cli::parse_from(["axon", "setup", "repair", "--migrate-env"]);
    let migrate_cfg =
        super::build_config::into_config(migrate).expect("setup repair --migrate-env should parse");
    assert_eq!(
        migrate_cfg.positional,
        vec!["repair".to_string(), "--migrate-env".to_string()]
    );
}
