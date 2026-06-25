use super::*;

fn test_paths() -> ServicePaths {
    ServicePaths {
        home: PathBuf::from("/home/j"),
        state_dir: PathBuf::from("/home/j/.local/state/axon"),
        env_path: PathBuf::from("/home/j/.config/axon/session-watch.env"),
        unit_path: PathBuf::from("/home/j/.config/systemd/user/session-watch-service.service"),
        axon_bin: PathBuf::from("/home/j/.local/bin/axon"),
        sqlite_path: PathBuf::from("/home/j/.axon/jobs.db"),
    }
}

#[test]
fn session_watch_env_file_uses_axon_names_and_no_ai_watch_name() {
    let env = session_watch_env_file(Path::new("/home/j/.axon/jobs.db"));
    assert!(env.contains("AXON_SQLITE_PATH=/home/j/.axon/jobs.db"));
    assert!(env.contains("RUST_LOG=warn"));
    assert!(!env.contains("CORTEX_"));
    assert!(!env.contains("ai-watch"));
}

#[test]
fn session_watch_service_unit_runs_sessions_watch_no_initial_scan() {
    let unit = session_watch_service_unit(
        Path::new("/home/j/.local/bin/axon"),
        Path::new("/home/j/.config/axon/session-watch.env"),
        Path::new("/home/j/.local/state/axon"),
        Path::new("/home/j"),
    );
    assert!(unit.contains("Description=axon real-time local AI session watch"));
    assert!(
        unit.contains("ExecStart=/home/j/.local/bin/axon sessions watch --no-initial-scan --json")
    );
    assert!(unit.contains("session-watch-service"));
    assert!(!unit.contains("axon-session-watch.service"));
    assert!(unit.contains("ProtectHome=tmpfs"));
    assert!(unit.contains("BindReadOnlyPaths=-/home/j/.local/bin/axon -/home/j/.config/axon/session-watch.env -/home/j/.claude/projects -/home/j/.codex/sessions -/home/j/.gemini/history -/home/j/.gemini/tmp"));
    assert!(unit.contains("ReadWritePaths=-/home/j/.local/state/axon -/home/j/.axon/jobs.db -/home/j/.axon/output -/home/j/.axon/logs -/home/j/.axon/artifacts -/home/j/.axon/screenshots -/home/j/.axon/chrome-diagnostics"));
    assert!(!unit.contains("ProtectHome=read-only"));
    assert!(!unit.contains("ReadWritePaths=/home/j/.axon /home/j/.local/state/axon"));
    assert!(!unit.contains("cortex"));
    assert!(!unit.contains("ai-watch-service"));
}

#[test]
fn session_watch_install_sequence_uses_initial_ingest_then_systemd_unit() {
    let paths = test_paths();
    assert_eq!(
        initial_ingest_command(&paths),
        CommandSpec::new(
            "/home/j/.local/bin/axon",
            ["sessions", "--wait", "true", "--json"]
        )
    );
    assert_eq!(
        daemon_reload_command(),
        CommandSpec::new("systemctl", ["--user", "daemon-reload"])
    );
    assert_eq!(
        enable_now_command(),
        CommandSpec::new(
            "systemctl",
            ["--user", "enable", "--now", "session-watch-service.service"]
        )
    );
}

#[test]
fn setup_check_status_warns_are_action_failures() {
    let warn = LocalSetupPhase {
        name: "service-active",
        status: LocalSetupStatus::Warn,
        detail: "inactive".to_string(),
        elapsed_ms: 0,
    };
    let ok = LocalSetupPhase {
        name: "write-files",
        status: LocalSetupStatus::Ok,
        detail: "ok".to_string(),
        elapsed_ms: 0,
    };

    assert!(phase_is_failure(SessionWatchServiceAction::Check, &warn));
    assert!(phase_is_failure(SessionWatchServiceAction::Status, &warn));
    assert!(!phase_is_failure(SessionWatchServiceAction::Install, &warn));
    assert!(!phase_is_failure(SessionWatchServiceAction::Check, &ok));
}

#[test]
fn initial_ingest_zero_chunks_is_error_when_session_files_exist() {
    let temp = tempfile::tempdir().unwrap();
    let session_dir = temp.path().join(".codex/sessions");
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(session_dir.join("session.jsonl"), "{}\n").unwrap();
    assert_eq!(command_json_chunks("{\"chunks_embedded\":0}"), Some(0));
    assert!(session_files_exist(temp.path()));
}
