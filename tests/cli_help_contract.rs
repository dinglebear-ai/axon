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
