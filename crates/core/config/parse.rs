mod build_config;
pub(crate) mod docker;
pub(crate) mod excludes;
pub(crate) mod helpers;
mod performance;
mod toml_config;

use super::cli::Cli;
use super::help::maybe_print_top_level_help_and_exit;
use super::types::Config;
use crate::crates::core::ui::report_error;
use clap::{Command, CommandFactory, Parser};

pub(crate) use docker::is_docker_service_host;

pub fn build_cli_command() -> Command {
    Cli::command()
}

pub fn parse_args() -> Config {
    maybe_print_top_level_help_and_exit();
    let cli = Cli::parse();
    match build_config::into_config(cli) {
        Ok(cfg) => cfg,
        Err(msg) => {
            report_error(&msg);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::docker::is_docker_service_host;
    use crate::crates::core::config::types::{CommandKind, McpTransport};
    use clap::Parser;
    use std::env;
    use std::sync::Mutex;

    /// Serializes tests that mutate process-wide environment variables.
    /// Prevents parallel test data races on `std::env::set_var` / `remove_var`.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

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
        const TRANSPORT: &str = "AXON_MCP_TRANSPORT";

        unsafe {
            env::set_var(PG, "postgresql://axon:postgres@127.0.0.1:53432/axon");
            env::set_var(REDIS, "redis://127.0.0.1:53379");
            env::set_var(AMQP, "amqp://axon:axonrabbit@127.0.0.1:45535/%2f");
            env::set_var(TRANSPORT, "http");
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
            env::remove_var(TRANSPORT);
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
        // ENV_LOCK serializes against the other MCP transport tests so they
        // can't race on the shared env var.
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
            "serve",
            "mcp",
        ]);
        let cfg = super::build_config::into_config(cli).expect("serve mcp config should parse");
        assert!(matches!(cfg.command, CommandKind::Mcp));
        assert_eq!(cfg.mcp_transport, McpTransport::Http);

        unsafe {
            env::remove_var(TRANSPORT);
        }
    }
}
