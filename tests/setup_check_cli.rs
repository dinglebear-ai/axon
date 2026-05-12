use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn axon_bin() -> &'static str {
    env!("CARGO_BIN_EXE_axon")
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
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

#[test]
fn setup_check_skips_mutation_and_warnings_are_nonfatal() {
    let home = tempfile::tempdir().unwrap();
    let fake_bin = tempfile::tempdir().unwrap();
    write_fake_setup_commands(fake_bin.path());

    let output = Command::new(axon_bin())
        .arg("setup")
        .arg("check")
        .env("HOME", home.path())
        .env("PATH", fake_path(fake_bin.path()))
        .env_remove("AXON_ENV_FILE")
        .env_remove("AXON_MCP_AUTH_MODE")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Skipped\tcompose-up"));
    assert!(stdout.contains("check mode does not start Docker services"));
    assert!(!home.path().join(".axon").exists());
}

#[test]
fn setup_check_returns_nonzero_after_printing_plain_error_report() {
    let home = tempfile::tempdir().unwrap();
    let fake_bin = tempfile::tempdir().unwrap();
    write_fake_setup_commands(fake_bin.path());

    let output = Command::new(axon_bin())
        .arg("setup")
        .arg("check")
        .env("HOME", home.path())
        .env("PATH", fake_path(fake_bin.path()))
        .env("AXON_TEST_DOCKER_FAIL", "1")
        .env_remove("AXON_ENV_FILE")
        .env_remove("AXON_MCP_AUTH_MODE")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("Error\tdocker"));
    assert!(stdout.contains("docker unavailable"));
    assert!(stderr.contains("axon setup completed with failed phases"));
}

#[test]
fn setup_check_returns_nonzero_after_printing_json_error_report() {
    let home = tempfile::tempdir().unwrap();
    let fake_bin = tempfile::tempdir().unwrap();
    write_fake_setup_commands(fake_bin.path());

    let output = Command::new(axon_bin())
        .arg("--json")
        .arg("setup")
        .arg("check")
        .env("HOME", home.path())
        .env("PATH", fake_path(fake_bin.path()))
        .env("AXON_TEST_DOCKER_FAIL", "1")
        .env_remove("AXON_ENV_FILE")
        .env_remove("AXON_MCP_AUTH_MODE")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["mode"], "check");
    assert_eq!(payload["has_errors"], true);
    assert!(
        payload["phases"]
            .as_array()
            .unwrap()
            .iter()
            .any(|phase| phase["name"] == "docker" && phase["status"] == "error")
    );
}
