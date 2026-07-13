use std::path::{Path, PathBuf};

use axon_core::config::RenderMode;
use serde_json::Value;

use super::{
    enforce_local_source_policy, enforce_network_source_policy,
    redact_local_path_for_public_payload,
};

#[tokio::test]
async fn network_sources_deny_private_redirects_for_http_and_chrome() {
    for render_mode in [RenderMode::Http, RenderMode::Chrome] {
        let err = run_source_fixture(
            "security/ssrf/redirect-private-ip.invalid.json",
            render_mode,
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "security.ssrf_denied");
    }
}

#[tokio::test]
async fn network_sources_deny_ssrf_fixture_pack_before_side_effects() {
    for fixture in [
        "security/ssrf/private-ip.invalid.json",
        "security/ssrf/dns-rebinding.invalid.json",
        "security/ssrf/loopback.invalid.json",
        "security/ssrf/link-local.invalid.json",
        "security/ssrf/file-scheme.invalid.json",
    ] {
        let err = run_source_fixture(fixture, RenderMode::Http)
            .await
            .unwrap_err();
        assert_eq!(err.code, "security.ssrf_denied", "{fixture}");
    }
}

#[tokio::test]
async fn local_source_denies_secret_paths_without_local_scope() {
    let err = run_local_fixture_without_scope("security/local/env-file.invalid.json")
        .await
        .unwrap_err();
    assert_eq!(err.code, "auth.scope_required");
}

#[tokio::test]
async fn local_source_denies_secret_paths_with_local_scope() {
    let value = read_fixture("security/local/env-file.invalid.json");

    let err = enforce_local_source_policy(value["path"].as_str().unwrap(), true)
        .expect_err("secret-like local paths are denied even with local scope");

    assert_eq!(err.code, "security.local_secret_denied");
}

#[tokio::test]
async fn local_source_denies_bare_env_file_with_local_scope() {
    let err = enforce_local_source_policy(".env", true)
        .expect_err("bare .env paths are denied before filesystem reads");

    assert_eq!(err.code, "security.local_secret_denied");
}

#[tokio::test]
async fn local_source_redacts_absolute_paths_from_public_payloads() {
    let value = read_fixture("security/local/env-file.invalid.json");
    let path = value["path"].as_str().unwrap();

    assert_eq!(
        redact_local_path_for_public_payload(path),
        "[redacted-local-path]"
    );
}

async fn run_source_fixture(
    fixture: &str,
    _render_mode: RenderMode,
) -> Result<(), super::SourceSecurityError> {
    let value = read_fixture(fixture);
    let requested_url = value["requested_url"].as_str().unwrap();
    let mut urls = vec![requested_url];
    if let Some(final_url) = value.get("final_url").and_then(Value::as_str) {
        urls.push(final_url);
    }
    enforce_network_source_policy(&urls)
}

async fn run_local_fixture_without_scope(fixture: &str) -> Result<(), super::SourceSecurityError> {
    let value = read_fixture(fixture);
    enforce_local_source_policy(value["path"].as_str().unwrap(), false)
}

fn read_fixture(fixture: &str) -> Value {
    let path = fixture_root().join(fixture);
    let bytes = std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .join("crates/axon-adapters/fixtures")
}
