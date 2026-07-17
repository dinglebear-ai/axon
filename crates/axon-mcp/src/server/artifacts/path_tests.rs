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
