use super::*;

fn source_input(input: &str) -> SourceProjectionInput {
    SourceProjectionInput {
        input: input.to_string(),
        idempotency_key: None,
    }
}

#[test]
fn code_search_rejects_source_idempotency_shape() {
    let value = serde_json::json!({
        "inputs": [{"input": "scheduler", "idempotency_key": "nope"}],
        "options": {"limit": 10}
    });

    assert!(serde_json::from_value::<CodeSearchRequest>(value).is_err());
}

#[test]
fn detached_batch_item_omits_input_echo() {
    let item = BatchItem::<SourceResult> {
        index: 0,
        input: None,
        outcome: BatchOutcome::Queued(JobDescriptor {
            kind: JobKind::Source,
            id: JobId::default(),
            status_url: "/v1/jobs/queued".to_string(),
            events_url: "/v1/jobs/queued/events".to_string(),
            stream_url: "/v1/jobs/queued/stream".to_string(),
            poll_after_ms: 250,
            cancel_url: None,
            retry_url: None,
            job_id: JobId::default(),
            status: LifecycleStatus::Queued,
            poll: None,
            created_at: None,
            updated_at: None,
        }),
    };

    assert!(serde_json::to_value(item).unwrap().get("input").is_none());
}

#[test]
fn scrape_projects_fixed_page_limits() {
    let request = ScrapeRequest {
        inputs: vec![source_input("https://example.test")],
        options: ScrapeOptions::default(),
    };

    let projected = project_scrape(&request).unwrap();

    assert_eq!(projected[0].scope, Some(SourceScope::Page));
    assert_eq!(projected[0].limits.max_pages, Some(1));
    assert_eq!(projected[0].limits.max_items, Some(1));
}

#[test]
fn crawl_embed_and_ingest_fix_their_semantics() {
    let crawl = project_crawl(&CrawlRequest {
        inputs: vec![source_input("https://example.test")],
        options: CrawlOptions::default(),
    })
    .unwrap();
    assert_eq!(crawl[0].scope, Some(SourceScope::Site));
    assert!(crawl[0].embed);

    let embed = project_embed(&EmbedRequest {
        inputs: vec![source_input("README.md")],
        options: EmbedOptions::default(),
    })
    .unwrap();
    assert!(embed[0].embed);

    let ingest = project_ingest(&IngestRequest {
        inputs: vec![source_input("README.md")],
        options: IngestOptions {
            no_embed: true,
            ..IngestOptions::default()
        },
    })
    .unwrap();
    assert!(!ingest[0].embed);
}

#[test]
fn projections_reject_empty_inputs_before_routing() {
    let error = project_scrape(&ScrapeRequest {
        inputs: Vec::new(),
        options: ScrapeOptions::default(),
    })
    .unwrap_err();

    assert_eq!(error.code.0, "projection.inputs_empty");
}

#[test]
fn code_search_plan_is_committed_state_only() {
    let plans = project_code_search(&CodeSearchRequest {
        inputs: vec![QueryProjectionInput {
            input: "projection registry".to_string(),
        }],
        options: CodeSearchProjectionOptions::default(),
    })
    .unwrap();

    assert_eq!(plans[0].content_kind, "code");
    let json = serde_json::to_value(&plans[0]).unwrap();
    assert!(json.get("ensure_fresh").is_none());
    assert!(json.get("cwd").is_none());
}
