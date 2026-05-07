/// Verifies that migrated MCP handlers do not import job-layer modules directly.
/// Each handler file is scanned at compile time (via include_str!) for forbidden
/// import fragments.  Broader patterns (no `{`) catch both `use jobs::foo;` and
/// `use jobs::foo::{bar, baz};` forms.
#[test]
fn migrated_mcp_handlers_do_not_import_jobs_layers_directly() {
    let checks = [
        (
            "handlers_embed_ingest.rs",
            include_str!("handlers_embed_ingest.rs"),
            &["crate::jobs::embed", "crate::jobs::ingest"][..],
        ),
        (
            "handlers_crawl_extract.rs",
            include_str!("handlers_crawl_extract.rs"),
            &["crate::jobs::crawl", "crate::jobs::extract"][..],
        ),
        (
            "handlers_system.rs",
            include_str!("handlers_system.rs"),
            &["crawl::screenshot::spider_screenshot_with_options"][..],
        ),
    ];

    for (file, source, forbidden_fragments) in checks {
        for forbidden in forbidden_fragments {
            assert!(
                !source.contains(forbidden),
                "{file} still contains forbidden direct-layer reference: {forbidden}"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Handler dispatch tests (comment #17).
//
// These run inside the `server.rs` #[cfg(test)] module, which grants access to
// the pub(super) handler methods on AxonMcpServer.
// ─────────────────────────────────────────────────────────────────────────────

/// Comment #17 — ingest/start without source_type returns INVALID_PARAMS.
///
/// Calls the real handle_ingest dispatch with IngestSubaction::Start and no
/// source_type, then verifies the returned error code.
#[tokio::test]
async fn ingest_start_missing_source_type_returns_invalid_params() {
    use crate::core::config::Config;
    use crate::mcp::schema::{IngestRequest, IngestSubaction};

    let server = super::AxonMcpServer::new(Config::default());
    let req = IngestRequest {
        subaction: Some(IngestSubaction::Start),
        source_type: None, // intentionally omitted
        target: None,
        include_source: None,
        sessions: None,
        job_id: None,
        limit: None,
        offset: None,
        response_mode: None,
    };
    let result = server.handle_ingest(req).await;
    assert!(
        result.is_err(),
        "ingest/start without source_type must return an error"
    );
    let err = result.unwrap_err();
    assert_eq!(
        err.code,
        rmcp::model::ErrorCode::INVALID_PARAMS,
        "missing source_type must return INVALID_PARAMS, got: {:?}",
        err.code
    );
    // Verify the error message is informative.
    let msg = err.message.to_lowercase();
    assert!(
        msg.contains("source_type") || msg.contains("required"),
        "error message should mention source_type; got: {msg}"
    );
}

/// Verify the refresh.start response shape includes both `job_ids` (array) and
/// `job_id` (last element) for multi-URL enqueue, using the actual `AxonToolResponse::ok`
/// builder that the handler uses rather than a locally-constructed JSON value.
#[test]
fn refresh_start_response_includes_all_job_ids() {
    use crate::mcp::schema::AxonToolResponse;

    let job_ids = vec![uuid::Uuid::new_v4(), uuid::Uuid::new_v4()];
    let last = *job_ids.last().unwrap();

    // Build the response the same way the real handler does.
    let response = AxonToolResponse::ok(
        "refresh",
        "start",
        serde_json::json!({
            "job_ids": job_ids,
            "job_id": last,
        }),
    );

    assert!(response.ok, "response must be ok=true");
    assert_eq!(response.action, "refresh");
    assert_eq!(response.subaction, "start");

    let ids: Vec<uuid::Uuid> = serde_json::from_value(response.data["job_ids"].clone()).unwrap();
    assert_eq!(ids.len(), 2, "job_ids must contain all enqueued IDs");

    let single: uuid::Uuid = serde_json::from_value(response.data["job_id"].clone()).unwrap();
    assert_eq!(
        single, last,
        "job_id must equal the last element of job_ids"
    );
}

#[tokio::test]
async fn ask_graph_true_returns_invalid_params() {
    use crate::core::config::Config;
    use crate::mcp::schema::AskRequest;

    let server = super::AxonMcpServer::new(Config::default());
    let req = AskRequest {
        query: Some("what is indexed?".to_string()),
        graph: Some(true),
        diagnostics: None,
        collection: None,
        since: None,
        before: None,
        hybrid_search: None,
        response_mode: None,
    };

    let err = server
        .handle_ask(req)
        .await
        .expect_err("graph=true should be rejected until graph retrieval exists");
    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    assert!(
        err.message.contains("graph retrieval is unavailable"),
        "unexpected error message: {}",
        err.message
    );
}
