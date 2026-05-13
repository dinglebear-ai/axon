use std::process::Command;

fn run_help(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_axon"))
        .args(args)
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

fn run_help_with_env(args: &[&str], envs: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_axon"));
    cmd.args(args);
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
        stdout.contains("Start MCP HTTP server runtime"),
        "expected top-level help to describe HTTP MCP runtime, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("Start MCP stdio server"),
        "top-level help still advertises stdio MCP runtime:\n{stdout}"
    );
}

#[test]
fn representative_help_hides_compatibility_and_graph_flags() {
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
            !stdout.contains("--lite"),
            "help output must not advertise lite compatibility: args={args:?}\n{stdout}"
        );
        assert!(
            !stdout.contains("Neo4j"),
            "help output must not advertise Neo4j: args={args:?}\n{stdout}"
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
        "--embed",
        "--tei-url",
        "--qdrant-url",
    ] {
        assert!(
            !stdout.contains(flag),
            "setup help should not include unrelated flag {flag}:\n{stdout}"
        );
    }
}
