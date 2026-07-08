use std::path::Path;
use std::process::Command;

fn axon_bin() -> &'static str {
    env!("CARGO_BIN_EXE_axon")
}

fn write_executable(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }
}

fn fake_path(dir: &Path) -> String {
    format!(
        "{}:{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

fn write_fake_setup_commands(dir: &Path) {
    write_executable(
        &dir.join("docker"),
        r#"#!/usr/bin/env sh
if [ "${AXON_TEST_DOCKER_FAIL:-0}" = "1" ]; then
  echo "docker unavailable" >&2
  exit 42
fi
if [ "${1:-}" = "compose" ] && [ "${2:-}" = "version" ]; then
  echo "Docker Compose version v2.0.0"
else
  echo "Docker version 26.0.0"
fi
"#,
    );
    write_executable(
        &dir.join("nvidia-smi"),
        "#!/usr/bin/env sh\necho 'RTX 4070'\n",
    );
    write_executable(
        &dir.join("gemini"),
        "#!/usr/bin/env sh\necho '0.0.0-test'\n",
    );
}

fn create_required_axon_dirs(home: &Path) {
    let axon = home.join(".axon");
    for child in [
        "output",
        "logs",
        "artifacts",
        "screenshots",
        "chrome-diagnostics",
        "lab-auth",
        "tei",
        "qdrant",
    ] {
        std::fs::create_dir_all(axon.join(child)).unwrap();
    }
}

fn assert_preflight_did_not_create_runtime_dirs(home: &Path) {
    let axon = home.join(".axon");
    for child in [
        "output",
        "artifacts",
        "screenshots",
        "chrome-diagnostics",
        "lab-auth",
        "tei",
        "qdrant",
    ] {
        assert!(
            !axon.join(child).exists(),
            "preflight must not create runtime dir {child}"
        );
    }
}

#[test]
fn preflight_skips_mutation_and_warnings_are_nonfatal() {
    let home = tempfile::tempdir().unwrap();
    let fake_bin = tempfile::tempdir().unwrap();
    write_fake_setup_commands(fake_bin.path());

    let output = Command::new(axon_bin())
        .arg("preflight")
        .env("HOME", home.path())
        .env("PATH", fake_path(fake_bin.path()))
        .env_remove("AXON_ENV_FILE")
        .env_remove("AXON_AUTH_MODE")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("◐ readiness"));
    assert!(stdout.contains("readiness skipped because prerequisite checks failed"));
    assert_preflight_did_not_create_runtime_dirs(home.path());
}

#[test]
fn preflight_warns_when_required_child_dirs_are_missing() {
    let home = tempfile::tempdir().unwrap();
    let fake_bin = tempfile::tempdir().unwrap();
    write_fake_setup_commands(fake_bin.path());
    std::fs::create_dir_all(home.path().join(".axon/logs")).unwrap();

    let output = Command::new(axon_bin())
        .arg("preflight")
        .env("HOME", home.path())
        .env("PATH", fake_path(fake_bin.path()))
        .env_remove("AXON_ENV_FILE")
        .env_remove("AXON_AUTH_MODE")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("⚠ filesystem"));
    assert!(stdout.contains("missing child directories"));
}

#[test]
fn preflight_reads_oauth_config_from_managed_env_file() {
    let home = tempfile::tempdir().unwrap();
    let fake_bin = tempfile::tempdir().unwrap();
    write_fake_setup_commands(fake_bin.path());
    create_required_axon_dirs(home.path());
    std::fs::write(
        home.path().join(".axon/.env"),
        "AXON_AUTH_MODE=oauth\nAXON_PUBLIC_URL=https://axon.example.com\n",
    )
    .unwrap();

    let output = Command::new(axon_bin())
        .arg("preflight")
        .env("HOME", home.path())
        .env("PATH", fake_path(fake_bin.path()))
        .env_remove("AXON_ENV_FILE")
        .env_remove("AXON_AUTH_MODE")
        .env_remove("AXON_GOOGLE_CLIENT_ID")
        .env_remove("AXON_GOOGLE_CLIENT_SECRET")
        .env_remove("AXON_AUTH_ADMIN_EMAIL")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("✗ oauth"));
    assert!(stdout.contains("AXON_GOOGLE_CLIENT_ID"));
}

#[test]
fn setup_skips_runtime_phases_after_prerequisite_errors() {
    let home = tempfile::tempdir().unwrap();
    let fake_bin = tempfile::tempdir().unwrap();
    write_fake_setup_commands(fake_bin.path());

    let output = Command::new(axon_bin())
        .arg("setup")
        .env("HOME", home.path())
        .env("PATH", fake_path(fake_bin.path()))
        .env("AXON_TEST_DOCKER_FAIL", "1")
        .env_remove("AXON_ENV_FILE")
        .env_remove("AXON_AUTH_MODE")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("✗ docker"));
    assert!(stdout.contains("◐ compose-up"));
    assert!(stdout.contains("compose startup skipped because prerequisite checks failed"));
}

#[test]
fn preflight_returns_nonzero_after_printing_plain_error_report() {
    let home = tempfile::tempdir().unwrap();
    let fake_bin = tempfile::tempdir().unwrap();
    write_fake_setup_commands(fake_bin.path());

    let output = Command::new(axon_bin())
        .arg("preflight")
        .env("HOME", home.path())
        .env("PATH", fake_path(fake_bin.path()))
        .env("AXON_TEST_DOCKER_FAIL", "1")
        .env_remove("AXON_ENV_FILE")
        .env_remove("AXON_AUTH_MODE")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("✗ docker"));
    assert!(stdout.contains("docker unavailable"));
    assert!(stderr.contains("axon preflight completed with failed phases"));
}

#[test]
fn preflight_returns_nonzero_after_printing_json_error_report() {
    let home = tempfile::tempdir().unwrap();
    let fake_bin = tempfile::tempdir().unwrap();
    write_fake_setup_commands(fake_bin.path());

    let output = Command::new(axon_bin())
        .arg("--json")
        .arg("preflight")
        .env("HOME", home.path())
        .env("PATH", fake_path(fake_bin.path()))
        .env("AXON_TEST_DOCKER_FAIL", "1")
        .env_remove("AXON_ENV_FILE")
        .env_remove("AXON_AUTH_MODE")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["mode"], "preflight");
    assert_eq!(payload["has_errors"], true);
    assert!(
        payload["phases"]
            .as_array()
            .unwrap()
            .iter()
            .any(|phase| phase["name"] == "docker" && phase["status"] == "error")
    );
}

#[test]
fn setup_plugin_hook_json_reports_policy_without_setup() {
    let home = tempfile::tempdir().unwrap();
    let fake_bin = tempfile::tempdir().unwrap();
    write_fake_setup_commands(fake_bin.path());

    let output = Command::new(axon_bin())
        .arg("--json")
        .arg("setup")
        .arg("plugin-hook")
        .arg("--no-setup")
        .env("HOME", home.path())
        .env("PATH", fake_path(fake_bin.path()))
        .env_remove("AXON_ENV_FILE")
        .env_remove("AXON_AUTH_MODE")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // The plugin-hook is probe-only (4.18.6+): it never deploys or runs setup,
    // it only reports whether the stack is already serving `/readyz`, emitting
    // `{exit_policy, stack}`. `stack` is "down" when unreachable (the usual case
    // under `--no-setup` in CI) or "already_healthy" when a stack is up.
    assert_eq!(payload["exit_policy"], "success");
    let stack = payload["stack"].as_str().unwrap_or_default();
    assert!(
        stack == "down" || stack == "already_healthy",
        "unexpected plugin-hook payload: {payload}"
    );
    assert_preflight_did_not_create_runtime_dirs(home.path());
}
