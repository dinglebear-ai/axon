use super::*;

#[test]
fn clone_argv_is_shallow_no_prompt_terminated() {
    let argv = clone_argv(
        "https://github.com/jmagar/axon.git",
        "/tmp/dest-xyz",
        "github.com:443:140.82.114.4",
    );
    assert_eq!(
        argv,
        vec![
            "-c".to_string(),
            "http.curloptResolve=github.com:443:140.82.114.4".to_string(),
            "-c".to_string(),
            "http.followRedirects=false".to_string(),
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
    let argv = clone_argv(
        "--upload-pack=evil",
        "/tmp/d",
        "example.com:443:93.184.216.34",
    );
    let dash_dash = argv.iter().position(|a| a == "--").expect("has terminator");
    let url = argv
        .iter()
        .position(|a| a == "--upload-pack=evil")
        .expect("url present");
    assert!(dash_dash < url, "-- must come before the URL argument");
}

#[test]
fn clone_argv_pins_validated_dns_and_disables_redirects() {
    let argv = clone_argv(
        "https://example.com/repo.git",
        "/tmp/d",
        "example.com:443:93.184.216.34,93.184.216.35",
    );
    assert!(
        argv.contains(
            &"http.curloptResolve=example.com:443:93.184.216.34,93.184.216.35".to_string()
        )
    );
    assert!(argv.contains(&"http.followRedirects=false".to_string()));
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
