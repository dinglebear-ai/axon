use super::*;
use crate::services::setup::{LocalSetupPhase, LocalSetupReport, LocalSetupStatus};
use std::path::PathBuf;

fn report_with_phase(name: &'static str, status: LocalSetupStatus) -> LocalSetupReport {
    LocalSetupReport {
        mode: "check",
        elapsed_ms: 0,
        target_seconds: 120,
        hard_max_seconds: 300,
        met_target: true,
        exceeded_hard_max: false,
        axon_home: PathBuf::from("/tmp/axon"),
        env_path: PathBuf::from("/tmp/axon/.env"),
        config_path: PathBuf::from("/tmp/axon/config.toml"),
        compose_dir: PathBuf::from("/tmp/axon/compose"),
        web_panel_url: "http://127.0.0.1:8001".to_string(),
        mcp_url: "http://127.0.0.1:8001/mcp".to_string(),
        has_errors: matches!(status, LocalSetupStatus::Error),
        phases: vec![LocalSetupPhase {
            name,
            status,
            detail: "phase detail".to_string(),
            elapsed_ms: 0,
        }],
    }
}

fn report_with(status: LocalSetupStatus) -> LocalSetupReport {
    report_with_phase("test", status)
}

#[test]
fn setup_failure_gate_rejects_error_reports() {
    assert!(fail_if_setup_failed(&report_with(LocalSetupStatus::Error)).is_err());
}

#[test]
fn setup_failure_gate_allows_warning_reports() {
    assert!(fail_if_setup_failed(&report_with(LocalSetupStatus::Warn)).is_ok());
}
