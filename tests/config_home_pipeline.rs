use std::process::Command;

#[test]
fn config_home_env_and_toml_are_loaded_before_command_parse() {
    let home = tempfile::tempdir().expect("temp home");
    let axon_home = home.path().join(".axon");
    std::fs::create_dir_all(&axon_home).expect("mkdir .axon");
    std::fs::write(
        axon_home.join(".env"),
        "QDRANT_URL=http://127.0.0.1:53333\nTEI_URL=http://127.0.0.1:52000\n",
    )
    .expect("write .env");
    std::fs::write(axon_home.join("config.toml"), "[tei]\nmax-retries = 4\n")
        .expect("write config");

    let output = Command::new(env!("CARGO_BIN_EXE_axon"))
        .env("HOME", home.path())
        .env_remove("AXON_ENV_FILE")
        .env_remove("AXON_CONFIG_PATH")
        .env_remove("QDRANT_URL")
        .env_remove("TEI_URL")
        // Force in-process execution: this test verifies that ~/.axon/.env +
        // config.toml are loaded during arg parsing — it does not require a
        // running `axon serve` instance, and falling through to client/server
        // mode causes spurious failures when port 8001 is unavailable or busy.
        .arg("--local")
        .arg("status")
        .arg("--json")
        .output()
        .expect("failed to execute axon status");

    assert!(
        output.status.success(),
        "expected config-home env+TOML pipeline to parse, status={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn symlinked_config_home_env_exits_before_repo_env_fallback() {
    let home = tempfile::tempdir().expect("temp home");
    let axon_home = home.path().join(".axon");
    std::fs::create_dir_all(&axon_home).expect("mkdir .axon");
    let target = home.path().join("attacker.env");
    std::fs::write(&target, "QDRANT_URL=http://127.0.0.1:1\n").expect("write target");
    std::os::unix::fs::symlink(&target, axon_home.join(".env")).expect("symlink .env");

    let output = Command::new(env!("CARGO_BIN_EXE_axon"))
        .env("HOME", home.path())
        .env_remove("AXON_ENV_FILE")
        .arg("status")
        .arg("--json")
        .output()
        .expect("failed to execute axon status");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("refusing to load symlinked .env"),
        "expected symlink hard-fail message, got:\n{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn unreadable_config_home_env_exits_before_repo_env_fallback() {
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().expect("temp home");
    let axon_home = home.path().join(".axon");
    std::fs::create_dir_all(&axon_home).expect("mkdir .axon");
    let env_path = axon_home.join(".env");
    std::fs::write(&env_path, "QDRANT_URL=http://127.0.0.1:1\n").expect("write .env");
    std::fs::set_permissions(&env_path, PermissionsExt::from_mode(0o000)).expect("chmod .env");

    let output = Command::new(env!("CARGO_BIN_EXE_axon"))
        .env("HOME", home.path())
        .env_remove("AXON_ENV_FILE")
        .arg("status")
        .arg("--json")
        .output()
        .expect("failed to execute axon status");

    std::fs::set_permissions(&env_path, PermissionsExt::from_mode(0o600)).expect("restore chmod");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot read") && stderr.contains("refusing to fall through"),
        "expected unreadable .env hard-fail message, got:\n{stderr}"
    );
}
