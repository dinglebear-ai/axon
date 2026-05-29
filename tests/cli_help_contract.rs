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
fn ingest_help_describes_target_argument() {
    let stdout = run_help(&["ingest", "--help"]);
    assert!(
        stdout.contains("Ingest target"),
        "expected ingest help to describe TARGET argument, got:\n{stdout}"
    );
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
            ("AXON_SERVER_URL", "http://secret-help-host:9999"),
            ("AXON_CHROME_REMOTE_URL", "http://secret-chrome:6000"),
        ],
    );
    assert!(
        !stdout.contains("secret_collection_for_help_test"),
        "help output leaked AXON_COLLECTION value:\n{stdout}"
    );
    assert!(
        !stdout.contains("secret-help-host"),
        "help output leaked AXON_SERVER_URL value:\n{stdout}"
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
fn embed_help_is_focused_on_embedding_and_jobs() {
    for args in [&["embed", "--help"][..], &["embed", "help"][..]] {
        let stdout = run_help(args);
        for expected in [
            "Embed file, directory, or URL into Qdrant",
            "axon embed [OPTIONS] [INPUT]",
            "status <job_id>",
            "--collection <name>",
            "--tei-url <url>",
            "--qdrant-url <url>",
        ] {
            assert!(
                stdout.contains(expected),
                "embed help missing {expected}: args={args:?}\n{stdout}"
            );
        }

        for unexpected in [
            "--start-url",
            "--max-depth",
            "--render-mode",
            "--skip-embed",
            "--chrome-remote-url",
            "--discover-sitemaps",
            "--research-depth",
            "--screenshot-full-page",
        ] {
            assert!(
                !stdout.contains(unexpected),
                "embed help should not include unrelated flag {unexpected}: args={args:?}\n{stdout}"
            );
        }
    }
}

#[test]
fn all_command_help_filters_inherited_global_noise() {
    for command in [
        "scrape",
        "crawl",
        "watch",
        "map",
        "extract",
        "search",
        "research",
        "embed",
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
        "dedupe",
        "ingest",
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
