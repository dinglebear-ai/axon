use super::*;

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

    assert_eq!(prepared.source_type, "codex_session");
    assert_eq!(prepared.content_type, "text");
    let extra = prepared.extra.expect("extra metadata");
    assert_eq!(extra["agent"], "codex");
    assert_eq!(
        extra["session_file"],
        "/home/me/.codex/sessions/2026/foo.jsonl"
    );
    assert_eq!(extra["project_name"], "axon_rust");
    assert_eq!(extra["model"], "gpt-5");
}

#[test]
fn prepared_session_request_rejects_oversized_total_text() {
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

    let err = request.validate(&cfg).expect_err("oversized request");
    assert!(err.contains("total prepared session text exceeds"));
}
