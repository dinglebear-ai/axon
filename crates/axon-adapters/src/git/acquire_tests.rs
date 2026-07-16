use super::*;

#[test]
fn clone_argv_is_shallow_no_prompt_terminated() {
    let argv = clone_argv("https://github.com/jmagar/axon.git", "/tmp/dest-xyz");
    assert_eq!(
        argv,
        vec![
            "clone".to_string(),
            "--depth=1".to_string(),
            "--no-tags".to_string(),
            "--".to_string(),
            "https://github.com/jmagar/axon.git".to_string(),
            "/tmp/dest-xyz".to_string(),
        ]
    );
}

#[test]
fn clone_argv_terminates_flag_shaped_urls() {
    // The `--` terminator must precede the URL so a hostile flag-shaped
    // argument is treated as a positional, never a git option.
    let argv = clone_argv("--upload-pack=evil", "/tmp/d");
    let dash_dash = argv.iter().position(|a| a == "--").expect("has terminator");
    let url = argv
        .iter()
        .position(|a| a == "--upload-pack=evil")
        .expect("url present");
    assert!(dash_dash < url, "-- must come before the URL argument");
}

#[tokio::test]
async fn clone_git_repo_rejects_ssrf_target() {
    // A loopback/private target is rejected before any git process is spawned.
    let err = clone_git_repo("https://127.0.0.1/secret.git")
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("refusing to clone"),
        "expected SSRF rejection, got: {err}"
    );
}
