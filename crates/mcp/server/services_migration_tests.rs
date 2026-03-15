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
            &["crate::crates::jobs::embed", "crate::crates::jobs::ingest"][..],
        ),
        (
            "handlers_crawl_extract.rs",
            include_str!("handlers_crawl_extract.rs"),
            &["crate::crates::jobs::crawl", "crate::crates::jobs::extract"][..],
        ),
        (
            "handlers_refresh_status.rs",
            include_str!("handlers_refresh_status.rs"),
            &["crate::crates::jobs::refresh"][..],
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
// Handler dispatch tests (comments #15 and #17).
//
// These run inside the `server.rs` #[cfg(test)] module, which grants access to
// the pub(super) handler methods on AxonMcpServer.
// ─────────────────────────────────────────────────────────────────────────────

/// Comment #15 — refresh/schedule with an unknown schedule_subaction returns INVALID_PARAMS.
///
/// Calls the real handle_refresh dispatch with RefreshSubaction::Schedule and an
/// unrecognised schedule_subaction value, then verifies the returned error code.
#[tokio::test]
async fn refresh_schedule_unknown_subaction_returns_invalid_params() {
    use crate::crates::core::config::Config;
    use crate::crates::mcp::schema::{RefreshRequest, RefreshSubaction};

    let server = super::AxonMcpServer::new(Config::default());
    let req = RefreshRequest {
        subaction: Some(RefreshSubaction::Schedule),
        url: None,
        urls: None,
        job_id: None,
        schedule_subaction: Some("launch_rockets".to_string()),
        schedule_name: None,
        limit: None,
        offset: None,
        response_mode: None,
    };
    let result = server.handle_refresh(req).await;
    assert!(
        result.is_err(),
        "unknown schedule_subaction must return an error"
    );
    let err = result.unwrap_err();
    assert_eq!(
        err.code,
        rmcp::model::ErrorCode::INVALID_PARAMS,
        "unknown schedule_subaction must return INVALID_PARAMS, got: {:?}",
        err.code
    );
    // Verify the error message names the unknown value.
    let msg = err.message.to_lowercase();
    assert!(
        msg.contains("launch_rockets") || msg.contains("unknown"),
        "error message should identify the unknown subaction; got: {msg}"
    );
}

/// Comment #17 — ingest/start without source_type returns INVALID_PARAMS.
///
/// Calls the real handle_ingest dispatch with IngestSubaction::Start and no
/// source_type, then verifies the returned error code.
#[tokio::test]
async fn ingest_start_missing_source_type_returns_invalid_params() {
    use crate::crates::core::config::Config;
    use crate::crates::mcp::schema::{IngestRequest, IngestSubaction};

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
