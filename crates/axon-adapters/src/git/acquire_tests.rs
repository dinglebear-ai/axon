use super::*;

#[test]
fn clone_argv_is_shallow_no_prompt_terminated() {
    let argv = clone_argv(
        "https://github.com/jmagar/axon.git",
        "/tmp/dest-xyz",
        "github.com:443:140.82.114.4",
        None,
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
        None,
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
        None,
    );
    assert!(
        argv.contains(
            &"http.curloptResolve=example.com:443:93.184.216.34,93.184.216.35".to_string()
        )
    );
    assert!(argv.contains(&"http.followRedirects=false".to_string()));
}

#[test]
fn github_credentials_are_scoped_to_https_github() {
    assert_eq!(
        credential_env_for_url("https://github.com/unraid/core"),
        Some(GITHUB_TOKEN_ENV)
    );
    assert_eq!(
        credential_env_for_url("https://GitHub.com/unraid/core.git"),
        Some(GITHUB_TOKEN_ENV)
    );
    assert_eq!(
        credential_env_for_url("http://github.com/unraid/core"),
        None
    );
    assert_eq!(
        credential_env_for_url("https://github.com:8443/unraid/core"),
        None
    );
    assert_eq!(
        credential_env_for_url("https://github.example.com/unraid/core"),
        None
    );
    assert_eq!(
        credential_env_for_url("https://gitlab.com/unraid/core"),
        None
    );
}

#[test]
fn github_clone_argv_uses_env_backed_credentials_without_embedding_secret() {
    let argv = clone_argv(
        "https://github.com/unraid/core",
        "/tmp/dest",
        "github.com:443:140.82.114.4",
        Some(GITHUB_TOKEN_ENV),
    );
    let joined = argv.join(" ");

    assert!(joined.contains("credential.helper="));
    assert!(joined.contains("credential.https://github.com.helper="));
    assert!(joined.contains("username=x-access-token"));
    assert!(joined.contains("password=$GITHUB_TOKEN"));
    assert!(!joined.contains("https://x-access-token:"));
    assert!(!joined.contains("ghp_"));

    let helper = argv
        .iter()
        .position(|arg| arg.contains("credential.https://github.com.helper="))
        .expect("credential helper config present");
    let clone = argv
        .iter()
        .position(|arg| arg == "clone")
        .expect("clone verb");
    assert!(helper < clone);
}

#[test]
fn github_credential_helper_is_consumed_by_git_without_url_secret() {
    use std::io::Write as _;

    let synthetic_token = "synthetic-private-repo-token";
    let helper = credential_helper_config(GITHUB_TOKEN_ENV);
    let mut child = std::process::Command::new("git")
        .args([
            "-c",
            "credential.helper=",
            "-c",
            &helper,
            "credential",
            "fill",
        ])
        .env(GITHUB_TOKEN_ENV, synthetic_token)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("git must be installed for git adapter tests");

    child
        .stdin
        .take()
        .expect("credential fill stdin")
        .write_all(b"protocol=https\nhost=github.com\n\n")
        .expect("write credential query");
    let output = child.wait_with_output().expect("credential fill completes");
    assert!(
        output.status.success(),
        "credential helper failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("credential output is utf-8");
    assert!(stdout.contains("username=x-access-token"));
    assert!(stdout.contains(&format!("password={synthetic_token}")));
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

#[cfg(unix)]
fn executable_script(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
async fn wait_for_file(path: &std::path::Path) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fake git must publish its process evidence");
}

#[cfg(unix)]
fn recorded_path(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_to_string(path).unwrap().trim().into()
}

#[cfg(unix)]
fn process_exists(pid: i32) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(unix)]
async fn assert_process_exits(pid: i32) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while process_exists(pid) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("process {pid} survived clone cancellation"));
}

#[cfg(unix)]
#[tokio::test]
async fn clone_success_returns_owned_checkout_and_drop_removes_it() {
    let fixture = tempfile::tempdir().unwrap();
    let destination_file = fixture.path().join("destination");
    let script = executable_script(
        fixture.path(),
        "fake-git-success",
        &format!(
            "for argument in \"$@\"; do destination=$argument; done\n\
             printf '%s\\n' \"$destination\" > '{}'\n\
             mkdir -p \"$destination/.git\"",
            destination_file.display()
        ),
    );

    let checkout =
        clone_git_repo_with_program("https://93.184.216.34/repo.git", script.as_os_str())
            .await
            .expect("fake clone succeeds");
    let destination = recorded_path(&destination_file);
    assert_eq!(checkout.path(), destination);
    assert!(destination.join(".git").is_dir());

    drop(checkout);
    assert!(!destination.exists(), "TempDir drop must remove checkout");
}

#[cfg(unix)]
#[tokio::test]
async fn clone_failure_removes_partial_checkout() {
    let fixture = tempfile::tempdir().unwrap();
    let destination_file = fixture.path().join("destination");
    let script = executable_script(
        fixture.path(),
        "fake-git-failure",
        &format!(
            "for argument in \"$@\"; do destination=$argument; done\n\
             printf '%s\\n' \"$destination\" > '{}'\n\
             mkdir -p \"$destination/.git\"\n\
             echo 'controlled clone failure' >&2\n\
             exit 7",
            destination_file.display()
        ),
    );

    let error = clone_git_repo_with_program("https://93.184.216.34/repo.git", script.as_os_str())
        .await
        .expect_err("fake clone fails");
    assert!(error.to_string().contains("git clone failed"));
    assert!(
        !recorded_path(&destination_file).exists(),
        "failed clone must remove its partial checkout"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn clone_cancellation_kills_process_group_and_removes_partial_checkout() {
    let fixture = tempfile::tempdir().unwrap();
    let destination_file = fixture.path().join("destination");
    let parent_pid_file = fixture.path().join("parent.pid");
    let child_pid_file = fixture.path().join("child.pid");
    let script = executable_script(
        fixture.path(),
        "fake-git-blocked",
        &format!(
            "for argument in \"$@\"; do destination=$argument; done\n\
             printf '%s\\n' \"$destination\" > '{destination}'\n\
             printf '%s\\n' \"$$\" > '{parent}'\n\
             sleep 30 &\n\
             child=$!\n\
             printf '%s\\n' \"$child\" > '{child}'\n\
             mkdir -p \"$destination/.git\"\n\
             wait \"$child\"",
            destination = destination_file.display(),
            parent = parent_pid_file.display(),
            child = child_pid_file.display(),
        ),
    );
    let task_script = script.clone();
    let clone = tokio::spawn(async move {
        clone_git_repo_with_program("https://93.184.216.34/repo.git", task_script.as_os_str()).await
    });
    wait_for_file(&child_pid_file).await;
    let destination = recorded_path(&destination_file);
    let parent_pid = std::fs::read_to_string(&parent_pid_file)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    let child_pid = std::fs::read_to_string(&child_pid_file)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    assert!(process_exists(parent_pid));
    assert!(process_exists(child_pid));

    clone.abort();
    assert!(clone.await.unwrap_err().is_cancelled());
    assert_process_exits(parent_pid).await;
    assert_process_exits(child_pid).await;
    assert!(
        !destination.exists(),
        "canceled clone must remove its partial checkout"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn clone_timeout_reaps_process_group_before_removing_partial_checkout() {
    let fixture = tempfile::tempdir().unwrap();
    let destination_file = fixture.path().join("timeout-destination");
    let parent_pid_file = fixture.path().join("timeout-parent.pid");
    let child_pid_file = fixture.path().join("timeout-child.pid");
    let script = executable_script(
        fixture.path(),
        "fake-git-timeout",
        &format!(
            "for argument in \"$@\"; do destination=$argument; done\n\
             printf '%s\\n' \"$destination\" > '{destination}'\n\
             printf '%s\\n' \"$$\" > '{parent}'\n\
             sleep 30 &\n\
             child=$!\n\
             printf '%s\\n' \"$child\" > '{child}'\n\
             mkdir -p \"$destination/.git\"\n\
             wait \"$child\"",
            destination = destination_file.display(),
            parent = parent_pid_file.display(),
            child = child_pid_file.display(),
        ),
    );

    let task_script = script.clone();
    let clone = tokio::spawn(async move {
        clone_git_repo_with_program_and_timeout(
            "https://93.184.216.34/repo.git",
            task_script.as_os_str(),
            Duration::from_secs(5),
        )
        .await
    });
    wait_for_file(&child_pid_file).await;
    let error = clone
        .await
        .expect("clone task joins")
        .expect_err("fake clone times out");
    assert!(error.to_string().contains("timed out"));
    let destination = recorded_path(&destination_file);
    let parent_pid = std::fs::read_to_string(&parent_pid_file)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    let child_pid = std::fs::read_to_string(&child_pid_file)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    assert!(!process_exists(parent_pid), "timeout must reap git leader");
    assert!(
        !process_exists(child_pid),
        "timeout must terminate git helper"
    );
    assert!(
        !destination.exists(),
        "timeout must remove checkout after process teardown"
    );
}
