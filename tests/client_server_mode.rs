use httpmock::Method::{GET, POST};
use httpmock::MockServer;
use serde_json::json;
use std::process::{Command, Output};
use tempfile::TempDir;

fn axon_with_home(home: &TempDir, server_url: &str, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_axon"));
    cmd.env_clear()
        .env("HOME", home.path())
        .env("AXON_DATA_DIR", home.path().join(".axon"))
        .env("AXON_SERVER_URL", server_url)
        .env("QDRANT_URL", "http://127.0.0.1:9")
        .env("TEI_URL", "http://127.0.0.1:9")
        .args(args);
    cmd
}

fn output_text(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn status_uses_server_mode_and_renders_result_json() {
    let server = MockServer::start();
    let action = server.mock(|when, then| {
        when.method(GET).path("/v1/status");
        then.status(200)
            .json_body(json!({ "totals": { "crawl": 7, "extract": 0, "embed": 0, "ingest": 0 } }));
    });
    let home = TempDir::new().expect("temp home");

    let output = axon_with_home(&home, &server.base_url(), &["status", "--json"])
        .output()
        .expect("run axon");

    assert!(
        output.status.success(),
        "status should succeed\n{}",
        output_text(&output)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("status stdout json");
    assert_eq!(value["totals"]["crawl"], 7);
    action.assert_calls(1);
}

#[test]
fn scrape_json_sends_auth_header_and_preserves_host_output() {
    let server = MockServer::start();
    let action = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/scrape")
            .header("authorization", "Bearer test-token")
            .body_includes("https://example.com");
        then.status(200).json_body(json!({
            "url": "https://example.com",
            "artifact_handle": {
                "kind": "scrape",
                "relative_path": "output/scrape/example.md",
                "display_path": "/srv/axon/output/scrape/example.md",
                "bytes": 42,
                "line_count": 2,
                "job_id": null,
                "url": "https://example.com"
            }
        }));
    });
    let home = TempDir::new().expect("temp home");
    let host_output = home.path().join("host.md");

    let output = axon_with_home(
        &home,
        &server.base_url(),
        &[
            "scrape",
            "https://example.com",
            "--json",
            "--output",
            host_output.to_str().expect("utf8 path"),
        ],
    )
    .env("AXON_MCP_HTTP_TOKEN", "test-token")
    .output()
    .expect("run axon");

    assert!(
        output.status.success(),
        "scrape should succeed\n{}",
        output_text(&output)
    );
    assert!(
        !host_output.exists(),
        "server-mode scrape must not write host output"
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("scrape stdout json");
    assert_eq!(
        value["artifact_handle"]["relative_path"],
        "output/scrape/example.md"
    );
    action.assert_calls(1);
}

#[test]
fn dead_server_fails_without_local_scrape_write() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind free port");
    let url = format!("http://{}", listener.local_addr().expect("local addr"));
    drop(listener);
    let home = TempDir::new().expect("temp home");
    let host_output = home.path().join("dead-server.md");

    let output = axon_with_home(
        &home,
        &url,
        &[
            "scrape",
            "https://example.com",
            "--output",
            host_output.to_str().expect("utf8 path"),
        ],
    )
    .output()
    .expect("run axon");

    assert!(
        !output.status.success(),
        "dead server should fail\n{}",
        output_text(&output)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("start `axon serve`"), "{stderr}");
    assert!(stderr.contains("--local"), "{stderr}");
    assert!(
        !host_output.exists(),
        "dead server path must not create local output"
    );
}

#[test]
fn explicit_local_mode_bypasses_server_url() {
    let server = MockServer::start();
    let action = server.mock(|when, then| {
        when.method(GET).path("/v1/status");
        then.status(500).body("should not be called");
    });
    let home = TempDir::new().expect("temp home");

    let output = axon_with_home(&home, &server.base_url(), &["status", "--json", "--local"])
        .output()
        .expect("run axon");

    assert!(
        output.status.success(),
        "local status should succeed\n{}",
        output_text(&output)
    );
    action.assert_calls(0);
}
