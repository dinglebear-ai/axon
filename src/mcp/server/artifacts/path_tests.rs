use super::*;
use std::env;
use std::fs;
use tempfile::tempdir;

use super::ARTIFACT_ENV_TEST_LOCK as ENV_CWD_LOCK;

#[allow(unsafe_code)]
#[test]
fn ensure_artifact_root_uses_env_override_with_context_subdir() {
    let _guard = ENV_CWD_LOCK.lock().expect("lock poisoned");
    let tmp = tempdir().expect("tempdir");
    let override_path = tmp.path().join("custom-artifacts");
    // SAFETY: guarded by ENV_CWD_LOCK; no concurrent env mutation in this module.
    unsafe {
        env::set_var(MCP_ARTIFACT_DIR_ENV, &override_path);
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio rt");
    let root = rt.block_on(ensure_artifact_root()).expect("artifact root");
    // Context subdir is appended to the override path
    let expected = override_path.join(client_context_name());
    assert_eq!(root, expected);
    assert!(root.exists());
    // SAFETY: guarded by ENV_CWD_LOCK; no concurrent env mutation in this module.
    unsafe {
        env::remove_var(MCP_ARTIFACT_DIR_ENV);
    }
}

#[allow(unsafe_code)]
#[test]
fn ensure_artifact_root_falls_back_when_primary_root_is_invalid() {
    let _guard = ENV_CWD_LOCK.lock().expect("lock poisoned");
    let tmp = tempdir().expect("tempdir");
    // Point artifact dir at a path that can't be created (a file blocks mkdir).
    let blocked = tmp.path().join("blocked");
    fs::write(&blocked, b"not-a-directory").expect("create blocking file");
    let invalid_root = blocked.join("subdir");
    // SAFETY: guarded by ENV_CWD_LOCK; no concurrent env mutation in this module.
    unsafe {
        env::set_var(MCP_ARTIFACT_DIR_ENV, &invalid_root);
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio rt");
    let root = rt
        .block_on(ensure_artifact_root())
        .expect("artifact root fallback");
    let expected_fallback = fallback_artifact_root();
    assert_eq!(root, expected_fallback);
    assert!(root.exists());
    // SAFETY: guarded by ENV_CWD_LOCK; no concurrent env mutation in this module.
    unsafe {
        env::remove_var(MCP_ARTIFACT_DIR_ENV);
    }
}

#[allow(unsafe_code)]
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn build_artifact_path_uses_action_subdirectory() {
    let _guard = ENV_CWD_LOCK.lock().expect("lock poisoned");
    let tmp = tempdir().expect("tempdir");
    // SAFETY: guarded by ENV_CWD_LOCK; no concurrent env mutation in this module.
    unsafe {
        env::set_var(MCP_ARTIFACT_DIR_ENV, tmp.path());
    }

    let root = ensure_artifact_root().await.expect("artifact root");
    let path = build_artifact_path("crawl-status-1234", "json")
        .await
        .expect("artifact path");
    assert_eq!(path, root.join("crawl").join("status-1234.json"));

    // SAFETY: guarded by ENV_CWD_LOCK; no concurrent env mutation in this module.
    unsafe {
        env::remove_var(MCP_ARTIFACT_DIR_ENV);
    }
}

#[allow(unsafe_code)]
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn validate_artifact_path_rejects_relative_traversal() {
    let _guard = ENV_CWD_LOCK.lock().expect("lock poisoned");
    let tmp = tempdir().expect("tempdir");
    unsafe {
        env::set_var(MCP_ARTIFACT_DIR_ENV, tmp.path());
    }

    let err = validate_artifact_path("../outside.json")
        .await
        .expect_err("traversal must fail");
    assert!(err.message.contains("traversal"));

    unsafe {
        env::remove_var(MCP_ARTIFACT_DIR_ENV);
    }
}

#[cfg(unix)]
#[allow(unsafe_code)]
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn validate_artifact_path_rejects_symlink_escape() {
    let _guard = ENV_CWD_LOCK.lock().expect("lock poisoned");
    let tmp = tempdir().expect("tempdir");
    unsafe {
        env::set_var(MCP_ARTIFACT_DIR_ENV, tmp.path());
    }

    let root = ensure_artifact_root().await.expect("artifact root");
    let outside = tmp.path().join("outside.txt");
    fs::write(&outside, "outside").expect("outside file");
    std::os::unix::fs::symlink(&outside, root.join("escape.txt")).expect("symlink");

    let err = validate_artifact_path("escape.txt")
        .await
        .expect_err("symlink escape must fail");
    assert!(err.message.contains("inside"));

    unsafe {
        env::remove_var(MCP_ARTIFACT_DIR_ENV);
    }
}

#[cfg(unix)]
#[allow(unsafe_code)]
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn resolve_artifact_output_path_rejects_symlink_parent_escape() {
    let _guard = ENV_CWD_LOCK.lock().expect("lock poisoned");
    let tmp = tempdir().expect("tempdir");
    unsafe {
        env::set_var(MCP_ARTIFACT_DIR_ENV, tmp.path());
    }

    let root = ensure_artifact_root().await.expect("artifact root");
    let outside = tmp.path().join("outside");
    fs::create_dir_all(&outside).expect("outside dir");
    std::os::unix::fs::symlink(&outside, root.join("escape")).expect("symlink");

    let err = resolve_artifact_output_path("escape/shot.png")
        .await
        .expect_err("symlink parent escape must fail before write");
    assert!(err.message.contains("inside"));

    unsafe {
        env::remove_var(MCP_ARTIFACT_DIR_ENV);
    }
}
