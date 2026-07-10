use httpmock::Method::{GET, POST};
use httpmock::MockServer;
use std::process::Command;
use tempfile::NamedTempFile;

#[test]
fn query_with_diagnostics_emits_structured_diagnostics_on_error() {
    let sqlite = NamedTempFile::new().expect("temp sqlite path");
    // Isolate from the developer's real ~/.axon/config.toml, which may still use
    // pre-contract-rewrite section names and would hard-fail config parse before
    // the query path runs. An empty temp config parses as defaults. (An explicit
    // AXON_CONFIG_PATH pointing at a *missing* file hard-fails, so it must exist,
    // and the loader requires a `.toml` suffix.)
    let config = tempfile::Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp config path");
    let tei = MockServer::start();
    tei.mock(|when, then| {
        when.method(POST).path("/embed");
        then.status(200)
            .json_body(serde_json::json!([[0.1_f32, 0.2_f32, 0.3_f32, 0.4_f32]]));
    });
    // `derive_embedding_identity()` probes `/info` (model id) before any real
    // query embed runs; an unmocked `/info` 404s and falls back to the
    // 1024-dim default, which then mismatches the 4-dim vector `/embed`
    // actually returns above.
    tei.mock(|when, then| {
        when.method(GET).path("/info");
        then.status(200)
            .json_body(serde_json::json!({"model_id": "diag-test-model"}));
    });

    let qdrant = MockServer::start();
    let collection = "diag_test_collection";
    qdrant.mock(|when, then| {
        when.method(GET).path(format!("/collections/{collection}"));
        then.status(404);
    });

    // Point AXON_ENV_FILE at a nonexistent path so the binary does not load
    // ~/.axon/.env or a repo-root .env, which would inject live QDRANT_URL /
    // TEI_URL and cause the binary to route to a real service.
    let no_env_file = sqlite.path().with_extension("nonexistent.env");

    let output = Command::new(env!("CARGO_BIN_EXE_axon"))
        .env("AXON_SQLITE_PATH", sqlite.path())
        .env("AXON_ENV_FILE", &no_env_file)
        .env("AXON_CONFIG_PATH", config.path())
        // Clear any inherited service-URL env vars so CLI flags are authoritative.
        .env_remove("QDRANT_URL")
        .env_remove("TEI_URL")
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
        stderr.contains("vector.collection_not_found"),
        "expected collection-not-found diagnostics in stderr, got:\n{stderr}"
    );
}
