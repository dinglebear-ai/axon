use super::{decode_claude_project_path, decode_path_walk, normalize_git_remote_to_owner_repo};
use std::path::PathBuf;
use tempfile::TempDir;

fn mk(tmp: &TempDir, rel: &str) -> PathBuf {
    let p = tmp.path().join(rel);
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn decode_empty_returns_none() {
    assert!(decode_claude_project_path("").is_none());
    assert!(decode_claude_project_path("-").is_none());
}

#[test]
fn decode_simple_path_with_dashes() {
    let tmp = TempDir::new().unwrap();
    mk(&tmp, "home/user/workspace/unraid-api");
    // Construct dir name as Claude would: replace `/` with `-`
    // `/tmp/xxx/home/user/workspace/unraid-api` — we test decode_path_walk directly
    // to avoid hardcoding the system `/` root.
    let parts: Vec<String> = vec!["home", "user", "workspace", "unraid", "api"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let result = decode_path_walk(tmp.path(), &parts, 0);
    assert_eq!(
        result,
        Some(tmp.path().join("home/user/workspace/unraid-api"))
    );
}

#[test]
fn decode_prefers_dash_dir_over_underscore_when_both_exist() {
    // If a real `axon-rust` dir exists it should be found before `axon_rust` variant
    let tmp = TempDir::new().unwrap();
    mk(&tmp, "home/user/axon-rust");
    let parts: Vec<String> = vec!["home", "user", "axon", "rust"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let result = decode_path_walk(tmp.path(), &parts, 0);
    assert_eq!(result, Some(tmp.path().join("home/user/axon-rust")));
}

#[test]
fn decode_falls_back_to_underscore_variant() {
    // No `axon-rust`, but `axon_rust` exists — should find it
    let tmp = TempDir::new().unwrap();
    mk(&tmp, "home/user/axon_rust");
    let parts: Vec<String> = vec!["home", "user", "axon", "rust"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let result = decode_path_walk(tmp.path(), &parts, 0);
    assert_eq!(result, Some(tmp.path().join("home/user/axon_rust")));
}

#[test]
fn decode_literal_dash_via_double_dash_encoding() {
    // dir name `-home-user--my-project` → path `/home/user/-my-project`
    // After stripping leading `-` and replacing `--` with placeholder:
    // parts = ["home", "user", "-my-project"] (the `-` is restored from `--`)
    // But `decode_claude_project_path` splits on single `-` after placeholder substitution.
    let tmp = TempDir::new().unwrap();
    mk(&tmp, "home/user/-my-project");
    let parts: Vec<String> = vec!["home", "user", "-my-project"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let result = decode_path_walk(tmp.path(), &parts, 0);
    assert_eq!(result, Some(tmp.path().join("home/user/-my-project")));
}

#[test]
fn decode_returns_none_when_no_matching_dir() {
    let tmp = TempDir::new().unwrap();
    let parts: Vec<String> = vec!["home", "nobody", "nonexistent"]
        .into_iter()
        .map(str::to_string)
        .collect();
    assert!(decode_path_walk(tmp.path(), &parts, 0).is_none());
}

// --- normalize_git_remote_to_owner_repo ---

#[test]
fn normalize_https_plain() {
    assert_eq!(
        normalize_git_remote_to_owner_repo("https://github.com/owner/repo"),
        Some("owner/repo".to_string())
    );
}

#[test]
fn normalize_https_with_git_suffix() {
    assert_eq!(
        normalize_git_remote_to_owner_repo("https://github.com/owner/repo.git"),
        Some("owner/repo".to_string())
    );
}

#[test]
fn normalize_https_with_token_credential() {
    assert_eq!(
        normalize_git_remote_to_owner_repo("https://ghp_token123@github.com/owner/repo.git"),
        Some("owner/repo".to_string())
    );
}

#[test]
fn normalize_https_with_user_password_credential() {
    assert_eq!(
        normalize_git_remote_to_owner_repo("https://user:password@github.com/owner/repo.git"),
        Some("owner/repo".to_string())
    );
}

#[test]
fn normalize_ssh_git_at_format() {
    assert_eq!(
        normalize_git_remote_to_owner_repo("git@github.com:owner/repo.git"),
        Some("owner/repo".to_string())
    );
}

#[test]
fn normalize_ssh_git_at_no_git_suffix() {
    assert_eq!(
        normalize_git_remote_to_owner_repo("git@github.com:owner/repo"),
        Some("owner/repo".to_string())
    );
}

#[test]
fn normalize_returns_none_for_empty() {
    assert_eq!(normalize_git_remote_to_owner_repo(""), None);
}
