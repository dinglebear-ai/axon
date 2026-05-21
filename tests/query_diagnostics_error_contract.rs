use httpmock::Method::{GET, POST};
use httpmock::MockServer;
use std::process::Command;
use tempfile::NamedTempFile;

#[test]
fn query_with_diagnostics_emits_structured_diagnostics_on_error() {
    let sqlite = NamedTempFile::new().expect("temp sqlite path");
    let tei = MockServer::start();
    tei.mock(|when, then| {
        when.method(POST).path("/embed");
        then.status(200)
            .json_body(serde_json::json!([[0.1_f32, 0.2_f32, 0.3_f32, 0.4_f32]]));
    });

    let qdrant = MockServer::start();
    let collection = "diag_test_collection";
    qdrant.mock(|when, then| {
        when.method(GET).path(format!("/collections/{collection}"));
        then.status(404);
    });

    // Point AXON_ENV_FILE at a nonexistent path so the binary does not load
    // ~/.axon/.env or a repo-root .env, which would inject live QDRANT_URL /
    // AXON_SERVER_URL and cause the binary to route to a real service.
    let no_env_file = sqlite.path().with_extension("nonexistent.env");

    let output = Command::new(env!("CARGO_BIN_EXE_axon"))
        .env("AXON_SQLITE_PATH", sqlite.path())
        .env("AXON_ENV_FILE", &no_env_file)
        // Force local execution even if AXON_SERVER_URL leaks from the outer env.
        .env("AXON_LOCAL_MODE", "true")
        // Clear any inherited service-URL env vars so CLI flags are authoritative.
        .env_remove("QDRANT_URL")
        .env_remove("TEI_URL")
        .env_remove("AXON_SERVER_URL")
        .arg("--tei-url")
        .arg(tei.base_url())
        .arg("--qdrant-url")
        .arg(qdrant.base_url())
        .arg("--collection")
        .arg(collection)
        .arg("query")
        .arg("--diagnostics")
        .arg("diagnostics contract test")
        .output()
        .expect("failed to execute axon query command");

    assert!(
        !output.status.success(),
        "query should fail when collection does not exist"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Diagnostics:"),
        "expected diagnostics prefix in stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains("\"stage\":\"query_vector_search_dispatch\""),
        "expected dispatch stage diagnostics in stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains(&format!("\"collection\":\"{collection}\"")),
        "expected collection diagnostics in stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains("404"),
        "expected 404 diagnostics in stderr, got:\n{stderr}"
    );
}
