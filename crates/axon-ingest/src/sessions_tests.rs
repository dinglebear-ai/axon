use super::*;
use crate::sessions::watch::validate::{
    SessionProvider, SessionWatchRoots, validate_session_file_path,
};

fn home_provider_tempdir(relative_root: &str) -> tempfile::TempDir {
    let home = std::env::var_os("HOME").expect("HOME for session tests");
    let root = PathBuf::from(home).join(relative_root);
    std::fs::create_dir_all(&root).unwrap();
    tempfile::Builder::new()
        .prefix("axon-session-test-")
        .tempdir_in(root)
        .unwrap()
}

#[test]
fn validates_codex_file_only_under_canonical_codex_root() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let good = home.join(".codex/sessions/2026/06/11/session.jsonl");
    std::fs::create_dir_all(good.parent().unwrap()).unwrap();
    std::fs::write(&good, "{}\n").unwrap();

    let roots = SessionWatchRoots::for_home(home);
    let validated = validate_session_file_path(&roots, &good).unwrap();
    assert_eq!(validated.provider, SessionProvider::Codex);
    assert_eq!(validated.basename, "session.jsonl");
    assert!(
        !validated
            .redacted_display
            .contains(home.to_string_lossy().as_ref())
    );
}

#[test]
fn rejects_substring_match_outside_provider_roots() {
    let temp = tempfile::tempdir().unwrap();
    let fake = temp.path().join("tmp/.codex/sessions/session.jsonl");
    std::fs::create_dir_all(fake.parent().unwrap()).unwrap();
    std::fs::write(&fake, "{}\n").unwrap();
    let roots = SessionWatchRoots::for_home(temp.path().join("home"));
    assert!(validate_session_file_path(&roots, &fake).is_err());
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_watch_file() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let real = home.join("outside/session.jsonl");
    std::fs::create_dir_all(real.parent().unwrap()).unwrap();
    std::fs::write(&real, "{}\n").unwrap();
    let link = home.join(".codex/sessions/link.jsonl");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    symlink(&real, &link).unwrap();
    let roots = SessionWatchRoots::for_home(home);
    assert!(validate_session_file_path(&roots, &link).is_err());
}

#[tokio::test]
#[serial_test::serial]
async fn collect_prepared_session_file_doc_parses_claude_file() {
    let project_dir = home_provider_tempdir(".claude/projects");
    let file = project_dir.path().join("claude-1.jsonl");
    std::fs::write(
        &file,
        serde_json::json!({
            "type": "user",
            "timestamp": "2026-06-11T12:00:00Z",
            "cwd": "/tmp/axon",
            "message": { "content": "remember this claude session" }
        })
        .to_string()
            + "\n",
    )
    .unwrap();

    let cfg = Config {
        collection: "axon-test".to_string(),
        ..Config::default()
    };
    let doc = collect_prepared_session_file_doc(&cfg, &file)
        .await
        .unwrap()
        .expect("claude doc");

    assert_eq!(doc.session_platform, "claude");
    assert!(doc.text.contains("remember this claude session"));
    assert_eq!(doc.session_file, file.to_string_lossy());
    assert!(doc.url.starts_with("file://"));
}

#[tokio::test]
#[serial_test::serial]
async fn collect_prepared_session_file_doc_parses_codex_file() {
    let session_dir = home_provider_tempdir(".codex/sessions");
    let file = session_dir.path().join("codex-1.jsonl");
    std::fs::write(
        &file,
        serde_json::json!({
            "type": "session_meta",
            "payload": { "cwd": "/tmp/axon", "model": "gpt-5" }
        })
        .to_string()
            + "\n"
            + &serde_json::json!({
                "type": "response_item",
                "payload": {
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "remember this codex session" }]
                }
            })
            .to_string()
            + "\n",
    )
    .unwrap();

    let cfg = Config::default();
    let doc = collect_prepared_session_file_doc(&cfg, &file)
        .await
        .unwrap()
        .expect("codex doc");

    assert_eq!(doc.session_platform, "codex");
    assert!(doc.text.contains("remember this codex session"));
}

#[tokio::test]
#[serial_test::serial]
async fn collect_prepared_session_file_doc_filters_codex_date_tree_by_workspace_project() {
    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME for session tests"));
    let session_dir = home.join(".codex/sessions/2026/06/11");
    std::fs::create_dir_all(&session_dir).unwrap();
    let file = session_dir.join(format!("codex-date-tree-{}.jsonl", uuid::Uuid::new_v4()));
    std::fs::write(
        &file,
        serde_json::json!({
            "type": "session_meta",
            "payload": { "cwd": "/home/jmagar/workspace/axon", "model": "gpt-5" }
        })
        .to_string()
            + "\n"
            + &serde_json::json!({
                "type": "response_item",
                "payload": {
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "date tree codex project filter" }]
                }
            })
            .to_string()
            + "\n",
    )
    .unwrap();

    let cfg = Config {
        sessions_project: Some("axon".to_string()),
        ..Config::default()
    };
    let doc = collect_prepared_session_file_doc(&cfg, &file)
        .await
        .unwrap()
        .expect("codex doc matching workspace project");

    assert_eq!(doc.session_project.as_deref(), Some("axon"));
    assert!(doc.text.contains("date tree codex project filter"));

    let cfg = Config {
        sessions_project: Some("not-axon".to_string()),
        ..Config::default()
    };
    assert!(
        collect_prepared_session_file_doc(&cfg, &file)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[serial_test::serial]
async fn collect_prepared_session_file_doc_parses_gemini_project_metadata() {
    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME for session tests"));
    let gemini_root = home.join(".gemini");
    std::fs::create_dir_all(&gemini_root).unwrap();
    let project_dir = home_provider_tempdir(".gemini/history");
    let project_name = project_dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let chats_dir = project_dir.path().join("chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    let file = chats_dir.join("gemini-1.json");
    std::fs::write(
        &file,
        serde_json::json!({
            "messages": [{
                "type": "user",
                "content": [{ "text": "remember this gemini session" }]
            }]
        })
        .to_string(),
    )
    .unwrap();

    let cfg = Config {
        sessions_project: Some(project_name.clone()),
        ..Config::default()
    };
    let doc = collect_prepared_session_file_doc(&cfg, &file)
        .await
        .unwrap()
        .expect("gemini doc");

    assert_eq!(doc.session_platform, "gemini");
    assert_eq!(doc.session_project.as_deref(), Some(project_name.as_str()));
    assert!(doc.text.contains("remember this gemini session"));
}

#[tokio::test]
#[serial_test::serial]
async fn collect_prepared_session_file_doc_rejects_unsupported_file() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("notes.txt");
    std::fs::write(&file, "not a session").unwrap();
    let cfg = Config::default();

    let err = collect_prepared_session_file_doc(&cfg, &file)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("unsupported session file"));
}

#[test]
fn session_text_redacts_common_secret_tokens() {
    let redacted = redact_session_text(
        "OPENAI key sk-testsecret1234567890 and token github_pat_1234567890abcdef",
    );
    assert!(redacted.contains("[redacted-secret]"));
    assert!(!redacted.contains("sk-testsecret1234567890"));
    assert!(!redacted.contains("github_pat_1234567890abcdef"));
}

#[test]
fn prepared_session_doc_converts_to_prepared_doc_without_extra_override() {
    let doc = PreparedSessionDoc {
        url: "file:///home/me/.codex/sessions/2026/foo.jsonl".to_string(),
        title: Some("foo.jsonl".to_string()),
        text: "### USER:\nhello\n\n### ASSISTANT:\nworld".to_string(),
        session_platform: "codex".to_string(),
        session_project: Some("axon_rust".to_string()),
        session_date: Some("2026-05-23T20:19:38Z".to_string()),
        session_turn_count: Some(1),
        session_file: "/home/me/.codex/sessions/2026/foo.jsonl".to_string(),
        extra: serde_json::json!({
            "model": "gpt-5",
            "agent": "spoofed",
            "session_file": "/tmp/spoofed"
        }),
    };

    let prepared = doc.to_prepared_doc().expect("valid prepared doc");

    assert_eq!(prepared.source_type(), "codex_session");
    assert_eq!(prepared.content_type(), "text");
    let extra = prepared.extra().cloned().expect("extra metadata");
    assert_eq!(extra["agent"], "codex");
    assert_eq!(
        extra["session_file"],
        "/home/me/.codex/sessions/2026/foo.jsonl"
    );
    assert_eq!(extra["project_name"], "axon_rust");
    assert_eq!(extra["model"], "gpt-5");
}

#[test]
fn prepared_session_request_accepts_many_docs_up_to_per_doc_limit() {
    // Total aggregate text used to be capped at per_doc_limit * 4. That check was
    // removed — the per-doc limit is the right boundary; batching handles large counts.
    let cfg = Config::test_default();
    let per_doc_limit = session_ingest_max_bytes_for_config(&cfg);
    let request = IngestSessionsPreparedRequest {
        docs: vec![
            PreparedSessionDoc {
                url: "file:///tmp/a.jsonl".to_string(),
                title: None,
                text: "x".repeat(per_doc_limit),
                session_platform: "claude".to_string(),
                session_project: None,
                session_date: None,
                session_turn_count: None,
                session_file: "/tmp/a.jsonl".to_string(),
                extra: serde_json::json!({}),
            };
            5
        ],
        project: None,
        collection: None,
    };

    // Should succeed: each doc is within per-doc limit; aggregate is no longer checked.
    request
        .validate(&cfg)
        .expect("should accept per-doc-bounded request");
}

fn small_doc(idx: usize) -> PreparedSessionDoc {
    PreparedSessionDoc {
        url: format!("file:///tmp/{idx}.jsonl"),
        title: None,
        text: "hello world".to_string(),
        session_platform: "claude".to_string(),
        session_project: None,
        session_date: None,
        session_turn_count: None,
        session_file: format!("/tmp/{idx}.jsonl"),
        extra: serde_json::json!({}),
    }
}

#[test]
fn split_prepared_session_docs_chunks_by_count_and_validates() {
    let cfg = Config::test_default();
    let docs: Vec<PreparedSessionDoc> = (0..600).map(small_doc).collect();

    let batches = prepared::split_prepared_session_docs(docs, &cfg);

    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0].len(), MAX_PREPARED_SESSION_DOCS);
    assert_eq!(batches[1].len(), MAX_PREPARED_SESSION_DOCS);
    assert_eq!(batches[2].len(), 600 - 2 * MAX_PREPARED_SESSION_DOCS);
    for batch in &batches {
        let request = IngestSessionsPreparedRequest {
            docs: batch.clone(),
            project: None,
            collection: None,
        };
        request.validate(&cfg).expect("batch within limits");
    }
}

#[test]
fn split_prepared_session_docs_chunks_by_total_bytes() {
    let cfg = Config::test_default();
    let total_limit = session_ingest_max_bytes_for_config(&cfg).saturating_mul(4);
    // Each doc holds ~40% of the per-request byte budget, so only two fit per batch.
    let mut doc = small_doc(0);
    doc.text = "x".repeat(total_limit * 2 / 5);
    let docs = vec![doc.clone(), doc.clone(), doc.clone()];

    let batches = prepared::split_prepared_session_docs(docs, &cfg);

    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].len(), 2);
    assert_eq!(batches[1].len(), 1);
}
