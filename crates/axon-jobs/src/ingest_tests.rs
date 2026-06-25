use super::IngestJob;
use super::types::{IngestSource, source_type_label, target_label};
use crate::status::JobStatus;
use chrono::Utc;
use uuid::Uuid;

// -- source_type_label tests --

#[test]
fn source_type_label_github() {
    let source = IngestSource::Github {
        repo: "owner/repo".into(),
        include_source: true,
    };
    assert_eq!(source_type_label(&source), "github");
}

#[test]
fn source_type_label_reddit() {
    let source = IngestSource::Reddit {
        target: "r/rust".into(),
    };
    assert_eq!(source_type_label(&source), "reddit");
}

#[test]
fn source_type_label_youtube() {
    let source = IngestSource::Youtube {
        target: "https://youtube.com/watch?v=abc".into(),
    };
    assert_eq!(source_type_label(&source), "youtube");
}

#[test]
fn source_type_label_sessions() {
    let source = IngestSource::Sessions {
        sessions_claude: true,
        sessions_codex: false,
        sessions_gemini: false,
        sessions_project: None,
    };
    assert_eq!(source_type_label(&source), "sessions");
}

// -- target_label tests --

#[test]
fn target_label_github_returns_repo() {
    let source = IngestSource::Github {
        repo: "anthropics/claude-code".into(),
        include_source: false,
    };
    assert_eq!(target_label(&source), "anthropics/claude-code");
}

#[test]
fn target_label_sessions_all_when_no_flags() {
    let source = IngestSource::Sessions {
        sessions_claude: false,
        sessions_codex: false,
        sessions_gemini: false,
        sessions_project: None,
    };
    assert_eq!(target_label(&source), "all");
}

#[test]
fn target_label_sessions_multiple_flags() {
    let source = IngestSource::Sessions {
        sessions_claude: true,
        sessions_codex: false,
        sessions_gemini: true,
        sessions_project: None,
    };
    assert_eq!(target_label(&source), "claude,gemini");
}

#[test]
fn target_label_sessions_with_project() {
    let source = IngestSource::Sessions {
        sessions_claude: true,
        sessions_codex: true,
        sessions_gemini: false,
        sessions_project: Some("axon-rust".into()),
    };
    assert_eq!(target_label(&source), "claude,codex:axon-rust");
}

#[test]
fn target_label_sessions_all_with_project() {
    let source = IngestSource::Sessions {
        sessions_claude: false,
        sessions_codex: false,
        sessions_gemini: false,
        sessions_project: Some("my-project".into()),
    };
    assert_eq!(target_label(&source), "all:my-project");
}

// -- IngestJob::status() tests --

#[test]
fn ingest_job_status_parses_known_variants() {
    for (raw, expected) in [
        ("pending", JobStatus::Pending),
        ("running", JobStatus::Running),
        ("completed", JobStatus::Completed),
        ("failed", JobStatus::Failed),
        ("canceled", JobStatus::Canceled),
    ] {
        let job = IngestJob {
            id: Uuid::nil(),
            status: raw.to_string(),
            source_type: "github".into(),
            target: "test".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            started_at: None,
            finished_at: None,
            error_text: None,
            result_json: None,
            config_json: serde_json::json!({}),
        };
        assert_eq!(job.status(), Some(expected));
    }
}

#[test]
fn ingest_job_status_returns_none_for_unknown() {
    let job = IngestJob {
        id: Uuid::nil(),
        status: "bogus".to_string(),
        source_type: "github".into(),
        target: "test".into(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        started_at: None,
        finished_at: None,
        error_text: None,
        result_json: None,
        config_json: serde_json::json!({}),
    };
    assert_eq!(job.status(), None);
}
