use super::*;
use axon_api::QueryResult;

#[test]
fn projection_cli_batch_renderer_outputs_tagged_outcomes() {
    let result = BatchResult::<QueryResult> {
        batch_id: BatchId::new(uuid::Uuid::new_v4()),
        status: BatchStatus::Completed,
        items: vec![BatchItem {
            index: 0,
            input: Some("needle".to_string()),
            outcome: BatchOutcome::Completed(QueryResult { results: vec![] }),
        }],
        summary: BatchSummary {
            total: 1,
            completed: 1,
            queued: 0,
            failed: 0,
            canceled: 0,
        },
    };
    let json = serde_json::to_value(result).unwrap();
    assert_eq!(json["items"][0]["outcome"]["status"], "completed");
}

#[test]
fn projection_output_is_atomic_and_never_clobbers() {
    let directory = std::env::temp_dir().join(format!("axon-projection-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let output = directory.join("result.json");
    write_atomic_no_clobber(&output, b"first").unwrap();
    assert_eq!(fs::read(&output).unwrap(), b"first");
    assert!(write_atomic_no_clobber(&output, b"second").is_err());
    assert_eq!(fs::read(&output).unwrap(), b"first");
    fs::remove_file(output).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn scrape_config_uses_shared_projection_and_preserves_legacy_flags() {
    let mut cfg = Config::default();
    cfg.command = CommandKind::Scrape;
    cfg.positional = vec!["https://example.test/page".to_string()];
    cfg.collection = "docs".to_string();
    cfg.embed = false;
    cfg.scrape_inline = true;

    let requests = scrape_requests_from_config(&cfg).unwrap();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.scope, Some(SourceScope::Page));
    assert_eq!(request.limits.max_pages, Some(1));
    assert_eq!(request.limits.max_items, Some(1));
    assert_eq!(request.collection.as_deref(), Some("docs"));
    assert!(!request.embed);
    assert_eq!(request.output.response_mode, ResponseMode::Inline);
    assert_eq!(request.execution.mode, ExecutionMode::Foreground);
    assert!(!request.execution.detached);
}
