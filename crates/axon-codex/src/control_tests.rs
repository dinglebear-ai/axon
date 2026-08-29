use super::*;

fn disabled_config() -> ControlConfig {
    ControlConfig {
        enabled: false,
        codex_binary: PathBuf::new(),
        control_home: PathBuf::new(),
        request_timeout: Duration::from_millis(50),
        read_concurrency: 2,
        max_restart_backoff: Duration::from_secs(30),
    }
}

#[tokio::test]
async fn disabled_runtime_fails_closed_without_touching_paths() {
    let runtime = ControlRuntime::new(disabled_config()).unwrap();
    assert_eq!(runtime.status().state, ControlState::Disabled);
    assert_eq!(
        runtime
            .with_read(|| async { Ok::<_, String>(()) })
            .await
            .unwrap_err(),
        "codex control runtime is disabled"
    );
}

#[cfg(unix)]
#[test]
fn enabled_runtime_rejects_symlinked_home_and_binary() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    let binary = root_path.join("codex-real");
    fs::write(&binary, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let binary_link = root_path.join("codex");
    symlink(&binary, &binary_link).unwrap();
    let home = root_path.join("home-real");
    fs::create_dir(&home).unwrap();
    let home_link = root_path.join("home");
    symlink(&home, &home_link).unwrap();

    let mut config = disabled_config();
    config.enabled = true;
    config.codex_binary = binary_link;
    config.control_home = home.clone();
    assert!(
        validate_config(&config)
            .unwrap_err()
            .contains("non-symlink")
    );
    config.codex_binary = binary;
    config.control_home = home_link;
    assert!(
        validate_config(&config)
            .unwrap_err()
            .contains("non-symlink")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn lanes_are_bounded_and_time_out() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    let binary = root_path.join("codex");
    fs::write(&binary, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let home = root_path.join("home");
    fs::create_dir(&home).unwrap();
    let mut config = disabled_config();
    config.enabled = true;
    config.codex_binary = binary;
    config.control_home = home;
    config.request_timeout = Duration::from_millis(10);
    let runtime = ControlRuntime::new(config).unwrap();
    let error = runtime
        .with_mutation(|| async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok::<_, String>(())
        })
        .await
        .unwrap_err();
    assert_eq!(error, "codex control mutation timed out");
}

#[cfg(unix)]
#[tokio::test]
async fn restart_backoff_is_bounded_counted_and_opens_circuit() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    let binary = root_path.join("codex");
    fs::write(&binary, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let home = root_path.join("home");
    fs::create_dir(&home).unwrap();
    let mut config = disabled_config();
    config.enabled = true;
    config.codex_binary = binary;
    config.control_home = home;
    config.max_restart_backoff = Duration::from_millis(2);
    let runtime = ControlRuntime::new(config).unwrap();

    // Exercise the supervisor directly while retaining a disabled filesystem-free config.
    for attempt in 1..RESTART_FAILURE_LIMIT {
        runtime.record_restart_failure(format!("crash {attempt}"));
        let started = tokio::time::Instant::now();
        runtime.begin_restart().await.unwrap();
        assert!(started.elapsed() >= Duration::from_millis(2));
        assert_eq!(runtime.status().restart_count, attempt);
        assert_eq!(runtime.status().state, ControlState::Starting);
    }

    runtime.record_restart_failure("final crash");
    assert_eq!(runtime.status().state, ControlState::CircuitOpen);
    assert_eq!(
        runtime.begin_restart().await.unwrap_err(),
        "Codex control restart circuit is open"
    );
    assert_eq!(runtime.status().restart_count, RESTART_FAILURE_LIMIT - 1);
}

#[cfg(unix)]
#[tokio::test]
async fn successful_restart_resets_backoff_and_consecutive_failure_circuit() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    let binary = root_path.join("codex");
    fs::write(&binary, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let home = root_path.join("home");
    fs::create_dir(&home).unwrap();
    let mut config = disabled_config();
    config.enabled = true;
    config.codex_binary = binary;
    config.control_home = home;
    config.max_restart_backoff = Duration::from_secs(2);
    let runtime = ControlRuntime::new(config).unwrap();

    for failure in 1..RESTART_FAILURE_LIMIT {
        runtime.record_restart_failure(format!("failure {failure}"));
    }
    runtime.begin_restart().await.unwrap();
    runtime.mark_ready();

    runtime.record_restart_failure("intermittent failure after success");
    let started = tokio::time::Instant::now();
    runtime.begin_restart().await.unwrap();
    assert!(started.elapsed() < Duration::from_millis(300));
    assert_eq!(runtime.status().state, ControlState::Starting);
}
