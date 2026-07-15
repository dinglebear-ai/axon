use std::process::Command;

fn help_fixture(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/cli-help")
            .join(name),
    )
    .expect("failed to read help fixture")
}

fn run_help(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_axon"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to execute axon binary");
    assert!(
        output.status.success(),
        "axon command failed: status={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn assert_help_snapshot(args: &[&str], fixture: &str) {
    let actual = run_help(args);
    let expected = help_fixture(fixture);
    assert_eq!(
        actual, expected,
        "help snapshot drift for args={args:?}; update {fixture} only after reviewing CLI output"
    );
}

fn run_help_with_env(args: &[&str], envs: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_axon"));
    cmd.args(args);
    cmd.env("NO_COLOR", "1");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("failed to execute axon binary");
    assert!(
        output.status.success(),
        "axon command failed: status={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn run_error(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_axon"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to execute axon binary");
    assert!(
        !output.status.success(),
        "axon command unexpectedly succeeded: args={args:?}"
    );
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn memory_list_help_exposes_browse_filters() {
    let stdout = run_help(&["memory", "list", "--help"]);
    for expected in [
        "--project",
        "--repo",
        "--file",
        "--type",
        "--status",
        "--limit",
    ] {
        assert!(
            stdout.contains(expected),
            "expected memory list help to include {expected}, got:\n{stdout}"
        );
    }
}

#[test]
fn top_level_help_describes_http_mcp_runtime() {
    let stdout = run_help(&["--help"]);
    assert!(
        stdout.contains("Start MCP stdio or unified HTTP runtime"),
        "expected top-level help to describe MCP stdio/unified HTTP runtime, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("Start MCP stdio server"),
        "top-level help still advertises stdio MCP runtime:\n{stdout}"
    );
}

#[test]
fn endpoints_help_describes_discovery_flags() {
    let stdout = run_help(&["endpoints", "--help"]);
    for expected in [
        "Discover API endpoints",
        "--include-bundles",
        "--first-party-only",
        "--unique-only",
        "--max-scripts",
        "--max-scan-bytes",
        "--verify",
        "--capture-network",
    ] {
        assert!(
            stdout.contains(expected),
            "endpoints help missing {expected}:\n{stdout}"
        );
    }
}

#[test]
fn representative_help_hides_graph_flags() {
    for args in [
        &["--help"][..],
        &["setup", "--help"],
        &["crawl", "--help"],
        &["serve", "--help"],
        &["mcp", "--help"],
    ] {
        let stdout = run_help(args);
        assert!(
            !stdout.contains("--graph"),
            "help output must not advertise graph: args={args:?}\n{stdout}"
        );
        assert!(
            !stdout.contains("Neo4j"),
            "help output must not advertise Neo4j: args={args:?}\n{stdout}"
        );
    }
}

#[test]
fn removed_tuning_flags_are_rejected_by_clap() {
    for flag in [
        "--chrome-remote-url",
        "--respect-robots",
        "--request-timeout-ms",
        "--embed",
        "--cache-skip-browser",
        "--start-url",
        "--watchdog-stale-timeout-secs",
        "--sqlite-path",
        "--server-url",
        "--log-level",
        "--graph",
    ] {
        let stderr = run_error(&[flag, "1", "status"]);
        assert!(
            stderr.contains("unexpected argument"),
            "{flag} should be rejected by clap:\n{stderr}"
        );
    }
}

#[test]
fn help_does_not_print_current_env_values() {
    let stdout = run_help_with_env(
        &["crawl", "--help"],
        &[
            ("AXON_COLLECTION", "secret_collection_for_help_test"),
            ("AXON_HTTP_TOKEN", "secret-help-token-9999"),
            ("AXON_CHROME_REMOTE_URL", "http://secret-chrome:6000"),
        ],
    );
    assert!(
        !stdout.contains("secret_collection_for_help_test"),
        "help output leaked AXON_COLLECTION value:\n{stdout}"
    );
    assert!(
        !stdout.contains("secret-help-token"),
        "help output leaked AXON_HTTP_TOKEN value:\n{stdout}"
    );
    assert!(
        !stdout.contains("secret-chrome"),
        "help output leaked AXON_CHROME_REMOTE_URL value:\n{stdout}"
    );
}

#[test]
fn setup_help_is_not_polluted_by_crawl_or_vector_flags() {
    let stdout = run_help(&["setup", "--help"]);
    for flag in [
        "--max-depth",
        "--render-mode",
        "--skip-embed",
        "--tei-url",
        "--qdrant-url",
    ] {
        assert!(
            !stdout.contains(flag),
            "setup help should not include unrelated flag {flag}:\n{stdout}"
        );
    }
}

#[test]
fn legacy_session_watch_surfaces_are_removed() {
    for args in [
        &["sessions", "watch", "--help"][..],
        &["sessions", "watch-status", "--help"],
        &["sessions", "smoke-watch", "--help"],
        &["setup", "session-watch-service", "--help"],
    ] {
        let stderr = run_error(args);
        assert!(
            stderr.contains("unrecognized subcommand") || stderr.contains("invalid value"),
            "legacy session watch surface should be rejected for args={args:?}:\n{stderr}"
        );
    }

    let stdout = run_help(&["sessions", "--help"]);
    assert!(
        !stdout.contains("watch"),
        "sessions help should not advertise legacy watch commands:\n{stdout}"
    );
    assert!(
        !stdout.contains("watch-status"),
        "sessions help should not advertise watch-status:\n{stdout}"
    );
    assert!(
        !stdout.contains("smoke-watch"),
        "sessions help should not advertise smoke-watch:\n{stdout}"
    );

    let setup = run_help(&["setup", "--help"]);
    assert!(
        !setup.contains("session-watch-service"),
        "setup help should not advertise session-watch-service:\n{setup}"
    );
}

#[test]
fn setup_split_help_surfaces_are_focused() {
    let preflight = run_help(&["preflight", "--help"]);
    assert!(preflight.contains("Check host prerequisites and service readiness"));
    assert!(!preflight.contains("--max-depth"));

    let smoke = run_help(&["smoke", "--help"]);
    assert!(smoke.contains("Run crawl/ask smoke checks against the running stack"));
    assert!(!smoke.contains("--render-mode"));

    let compose = run_help(&["compose", "--help"]);
    for expected in ["up", "down", "restart", "rebuild"] {
        assert!(
            compose.contains(expected),
            "compose help missing {expected}:\n{compose}"
        );
    }

    let setup_init = run_help(&["setup", "init", "--help"]);
    for expected in [
        "--mcp-host",
        "--mcp-port",
        "--auth-mode",
        "--mcp-token",
        "--oauth-public-url",
        "--google-client-id",
        "--google-client-secret",
        "--auth-admin-email",
        "--tavily-api-key",
        "--github-token",
        "--reddit-client-id",
        "--reddit-client-secret",
    ] {
        assert!(
            setup_init.contains(expected),
            "setup init help missing {expected}:\n{setup_init}"
        );
    }
    assert!(
        !setup_init.contains("--no-repair"),
        "setup init help must not advertise removed repair flag:\n{setup_init}"
    );
    for unexpected in [
        "--max-depth",
        "--render-mode",
        "--skip-embed",
        "--tei-url",
        "--qdrant-url",
    ] {
        assert!(
            !setup_init.contains(unexpected),
            "setup init help should not include unrelated flag {unexpected}:\n{setup_init}"
        );
    }
}

#[test]
fn plugin_hook_no_repair_flag_is_removed() {
    let stderr = run_error(&["setup", "plugin-hook", "--no-repair"]);
    assert!(
        stderr.contains("unexpected argument"),
        "removed plugin hook flag should be rejected:\n{stderr}"
    );
}

#[test]
fn setup_split_help_snapshots_match() {
    assert_help_snapshot(&["preflight", "--help"], "preflight.help");
    assert_help_snapshot(&["smoke", "--help"], "smoke.help");
    assert_help_snapshot(&["compose", "--help"], "compose.help");
    assert_help_snapshot(&["setup", "init", "--help"], "setup-init.help");
}

#[test]
fn all_command_help_filters_inherited_global_noise() {
    // scrape/crawl/embed/ingest/code-search/dedupe were removed at the
    // Phase 10 clean-break cutover -- they no longer have distinct help
    // (any unrecognized positional falls through to the unified `source`
    // command, which legitimately has flags like `--embed` and
    // `--screenshot-full-page` that would false-positive against this
    // loop's "unexpected" noise list). `dedupe` was replaced by `prune`.
    for command in [
        "watch",
        "map",
        "extract",
        "search",
        "research",
        "debug",
        "doctor",
        "query",
        "retrieve",
        "ask",
        "evaluate",
        "train",
        "suggest",
        "sources",
        "domains",
        "stats",
        "status",
        "prune",
        "memory",
        "sessions",
        "screenshot",
        "completions",
        "preflight",
        "smoke",
        "compose",
        "serve",
        "setup",
        "mcp",
        "migrate",
        "config",
    ] {
        let stdout = run_help(&[command, "--help"]);
        for unexpected in [
            "--start-url",
            "--chrome-remote-url",
            "--chrome-proxy",
            "--chrome-user-agent",
            "--embed",
            "--cache-skip-browser",
            "--discover-sitemaps",
            "--watchdog-stale-timeout-secs",
            "--auto-switch-thin-ratio",
            "--screenshot-full-page",
        ] {
            assert!(
                !stdout.contains(unexpected),
                "{command} help should not include inherited global noise {unexpected}:\n{stdout}"
            );
        }
    }
}
