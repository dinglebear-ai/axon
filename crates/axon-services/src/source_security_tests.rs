use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{
    enforce_local_source_allowed_roots, enforce_local_source_policy,
    redact_local_path_for_public_payload,
};

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

#[test]
fn local_source_shared_denylist_blocks_credential_homes_and_keys_without_cloud_substring_false_positive()
 {
    for path in [
        "/home/user/.aws/credentials",
        "/home/user/.kube/config",
        "/home/user/.netrc",
        "/home/user/.ssh/id_rsa",
        "/home/user/secret.pem",
    ] {
        let err = enforce_local_source_policy(path, true)
            .expect_err("sensitive local path must be denied");
        assert_eq!(err.code, "security.local_secret_denied", "{path}");
    }

    assert!(
        enforce_local_source_policy("/home/user/projects/cloudflare-worker/README.md", true)
            .is_ok(),
        "ordinary paths containing the word cloud must not be denied"
    );
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

#[test]
fn local_source_allowed_roots_accept_exact_and_nested_paths() {
    let allowed = tempfile::tempdir().expect("allowed root");
    let nested = allowed.path().join("generation");
    std::fs::create_dir(&nested).expect("nested source");

    assert_eq!(
        enforce_local_source_allowed_roots(allowed.path(), &[allowed.path().to_path_buf()])
            .expect("exact root"),
        allowed.path()
    );
    assert_eq!(
        enforce_local_source_allowed_roots(&nested, &[allowed.path().to_path_buf()])
            .expect("nested root"),
        nested
    );
}

#[test]
fn local_source_allowed_roots_reject_empty_relative_parent_and_sibling_prefix() {
    let parent = tempfile::tempdir().expect("parent");
    let allowed = parent.path().join("young-office");
    let sibling = parent.path().join("young-office-private");
    std::fs::create_dir(&allowed).expect("allowed");
    std::fs::create_dir(&sibling).expect("sibling");

    let empty = enforce_local_source_allowed_roots(&allowed, &[]).expect_err("empty deny");
    assert_eq!(empty.code, "security.local_root_denied");

    let relative = enforce_local_source_allowed_roots(
        Path::new("young-office"),
        std::slice::from_ref(&allowed),
    )
    .expect_err("relative deny");
    assert_eq!(relative.code, "security.local_root_denied");

    let parent_escape = enforce_local_source_allowed_roots(
        &allowed.join("../young-office-private"),
        std::slice::from_ref(&allowed),
    )
    .expect_err("parent escape deny");
    assert_eq!(parent_escape.code, "security.local_root_denied");

    let sibling_prefix =
        enforce_local_source_allowed_roots(&sibling, std::slice::from_ref(&allowed))
            .expect_err("sibling prefix deny");
    assert_eq!(sibling_prefix.code, "security.local_root_denied");

    for error in [empty, relative, parent_escape, sibling_prefix] {
        assert_eq!(
            error.message,
            "local source is outside configured allowed roots"
        );
        assert!(!error.message.contains(&parent.path().display().to_string()));
    }
}

#[cfg(unix)]
#[test]
fn local_source_allowed_roots_reject_symlinked_request_root() {
    let allowed = tempfile::tempdir().expect("allowed root");
    let real = allowed.path().join("real");
    let linked = allowed.path().join("current");
    std::fs::create_dir(&real).expect("real source");
    std::os::unix::fs::symlink(&real, &linked).expect("source symlink");

    let error = enforce_local_source_allowed_roots(&linked, &[allowed.path().to_path_buf()])
        .expect_err("symlinked source root must be denied");

    assert_eq!(error.code, "security.local_root_denied");
    assert_eq!(
        error.message,
        "local source is outside configured allowed roots"
    );
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
