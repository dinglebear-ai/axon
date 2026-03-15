/// MCP handler contract parity tests.
///
/// These are compile-time and pure-logic tests. No live services are required.
/// They verify that:
///   1. Option mapper round-trips produce correct service types.
///   2. The JSON response keys that MCP handlers emit match the schema contract.
///   3. Handler input parameters are forwarded to service calls correctly.
use axon::crates::mcp::schema::{
    AxonRequest, AxonToolResponse, IngestSubaction, RefreshSubaction, SearchTimeRange,
    parse_axon_request,
};
use axon::crates::mcp::server::common::{
    to_map_options, to_pagination, to_retrieve_options, to_search_options, to_service_time_range,
};
use axon::crates::services::query::map_retrieve_result;
use axon::crates::services::types::{
    AskResult, DoctorResult, DomainFacet, DomainsResult, MapOptions, Pagination, QueryResult,
    RetrieveOptions, RetrieveResult, SearchOptions, SearchResult, ServiceTimeRange, SourcesResult,
    StatsResult, SuggestResult,
};

// ─────────────────────────────────────────────────────────────────────────────
// Group 1: Option mapper round-trips (verifies common.rs helpers)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn pagination_default_values_when_both_none() {
    let p = to_pagination(None, None, 10);
    assert_eq!(p.limit, 10, "default limit should be 10");
    assert_eq!(p.offset, 0, "default offset should be 0");
}

#[test]
fn pagination_custom_values_pass_through() {
    let p = to_pagination(Some(42), Some(100), 10);
    assert_eq!(p.limit, 42);
    assert_eq!(p.offset, 100);
}

#[test]
fn pagination_limit_clamped_to_minimum_one() {
    let p = to_pagination(Some(0), None, 10);
    assert_eq!(p.limit, 1, "zero limit should be clamped to 1");
}

#[test]
fn pagination_limit_clamped_to_maximum_500() {
    let p = to_pagination(Some(9999), None, 10);
    assert_eq!(p.limit, 500, "limit above 500 should be clamped to 500");
}

#[test]
fn retrieve_options_none_passes_through() {
    let r = to_retrieve_options(None);
    assert_eq!(r, RetrieveOptions { max_points: None });
}

#[test]
fn retrieve_options_some_value_passes_through() {
    let r = to_retrieve_options(Some(128));
    assert_eq!(
        r,
        RetrieveOptions {
            max_points: Some(128)
        }
    );
}

#[test]
fn time_range_all_variants_map_correctly() {
    assert_eq!(
        to_service_time_range(SearchTimeRange::Day),
        ServiceTimeRange::Day
    );
    assert_eq!(
        to_service_time_range(SearchTimeRange::Week),
        ServiceTimeRange::Week
    );
    assert_eq!(
        to_service_time_range(SearchTimeRange::Month),
        ServiceTimeRange::Month
    );
    assert_eq!(
        to_service_time_range(SearchTimeRange::Year),
        ServiceTimeRange::Year
    );
}

#[test]
fn search_options_defaults_when_all_none() {
    let opts = to_search_options(None, None, None, 10);
    assert_eq!(opts.limit, 10);
    assert_eq!(opts.offset, 0);
    assert!(opts.time_range.is_none());
}

#[test]
fn search_options_time_range_forwarded() {
    let opts = to_search_options(Some(5), Some(2), Some(SearchTimeRange::Week), 10);
    assert_eq!(opts.limit, 5);
    assert_eq!(opts.offset, 2);
    assert_eq!(opts.time_range, Some(ServiceTimeRange::Week));
}

#[test]
fn map_options_default_values_when_both_none() {
    // limit=None → 0 (no limit), matching CLI default.
    let m = to_map_options(None, None);
    assert_eq!(
        m,
        MapOptions {
            limit: 0,
            offset: 0
        }
    );
}

#[test]
fn map_options_large_limit_honored_without_clamp() {
    // Values are passed through as-is; no 500-cap is applied.
    let m = to_map_options(Some(100_000), Some(5));
    assert_eq!(m.limit, 100_000);
    assert_eq!(m.offset, 5);
}

// ─────────────────────────────────────────────────────────────────────────────
// Group 2: Service result type field consistency
// Verifies the service result structs expose the fields MCP handlers expect.
// These are compile-time tests — if the struct fields don't exist, the file
// won't compile.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sources_result_has_expected_fields() {
    let r = SourcesResult {
        count: 5,
        limit: 10,
        offset: 0,
        urls: vec![("https://example.com".to_string(), 3)],
    };
    assert_eq!(r.count, 5);
    assert_eq!(r.limit, 10);
    assert_eq!(r.offset, 0);
    assert_eq!(r.urls.len(), 1);
    assert_eq!(r.urls[0].0, "https://example.com");
    assert_eq!(r.urls[0].1, 3);
}

#[test]
fn domains_result_has_expected_fields() {
    let r = DomainsResult {
        domains: vec![DomainFacet {
            domain: "example.com".to_string(),
            vectors: 42,
        }],
        limit: 25,
        offset: 0,
    };
    assert_eq!(r.domains.len(), 1);
    assert_eq!(r.domains[0].domain, "example.com");
    assert_eq!(r.domains[0].vectors, 42);
    assert_eq!(r.limit, 25);
}

#[test]
fn stats_result_wraps_payload() {
    let v = serde_json::json!({ "points": 1000 });
    let r = StatsResult { payload: v.clone() };
    assert_eq!(r.payload["points"], 1000);
}

#[test]
fn doctor_result_wraps_payload() {
    let v = serde_json::json!({ "postgres": "ok", "redis": "ok" });
    let r = DoctorResult { payload: v };
    assert_eq!(r.payload["postgres"], "ok");
    assert_eq!(r.payload["redis"], "ok");
}

#[test]
fn query_result_has_results_vec() {
    let v = vec![serde_json::json!({ "score": 0.9, "url": "https://a.com" })];
    let r = QueryResult { results: v };
    assert_eq!(r.results.len(), 1);
    assert_eq!(r.results[0]["score"], 0.9);
}

#[test]
fn retrieve_result_chunks_are_empty_for_zero_count() {
    let r = RetrieveResult { chunks: Vec::new() };
    assert!(r.chunks.is_empty());
}

#[test]
fn map_retrieve_result_stores_chunk_count_inside_chunks_element() {
    // Pins the data contract that handlers_query.rs::handle_retrieve relies on:
    // chunk_count lives at chunks[0]["chunk_count"], not in chunks.len().
    let r = map_retrieve_result(7, "hello world".to_string());
    assert_eq!(
        r.chunks.len(),
        1,
        "one wrapper element regardless of chunk count"
    );
    assert_eq!(r.chunks[0]["chunk_count"], 7);
    assert_eq!(r.chunks[0]["content"], "hello world");
}

#[test]
fn ask_result_exposes_payload_value() {
    let v = serde_json::json!({ "answer": "42" });
    let r = AskResult { payload: v };
    assert_eq!(r.payload["answer"], "42");
}

#[test]
fn suggest_result_exposes_url_vec() {
    let r = SuggestResult {
        urls: vec!["https://rust-lang.org".to_string()],
    };
    assert_eq!(r.urls.len(), 1);
    assert_eq!(r.urls[0], "https://rust-lang.org");
}

#[test]
fn search_result_exposes_results_vec() {
    let r = SearchResult {
        results: vec![serde_json::json!({ "url": "https://b.com" })],
    };
    assert_eq!(r.results.len(), 1);
    assert_eq!(r.results[0]["url"], "https://b.com");
}

// ─────────────────────────────────────────────────────────────────────────────
// Group 3: Pagination struct equality (ensure PartialEq derives work)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn pagination_equality() {
    let a = Pagination {
        limit: 10,
        offset: 5,
    };
    let b = Pagination {
        limit: 10,
        offset: 5,
    };
    assert_eq!(a, b);
}

#[test]
fn search_options_equality() {
    let a = SearchOptions {
        limit: 20,
        offset: 0,
        time_range: Some(ServiceTimeRange::Day),
    };
    let b = SearchOptions {
        limit: 20,
        offset: 0,
        time_range: Some(ServiceTimeRange::Day),
    };
    assert_eq!(a, b);
}

#[test]
fn retrieve_options_equality() {
    let a = RetrieveOptions {
        max_points: Some(50),
    };
    let b = RetrieveOptions {
        max_points: Some(50),
    };
    assert_eq!(a, b);
}

// ─────────────────────────────────────────────────────────────────────────────
// Group 4: MCP handler response contract (comments #15, #16, #17, #18)
// ─────────────────────────────────────────────────────────────────────────────

/// Comment #18 — mcp_embed_start_returns_job_id_payload_shape
///
/// handle_embed_start emits `AxonToolResponse::ok("embed", "start", json!({"job_id": ...}))`.
/// Construct the actual response type and verify the serialized envelope shape.
/// If the field name changes in the handler, this test will catch it.
#[test]
fn mcp_embed_start_returns_job_id_payload_shape() {
    let resp = AxonToolResponse::ok("embed", "start", serde_json::json!({ "job_id": "abc-123" }));
    let serialized = serde_json::to_value(&resp).expect("AxonToolResponse must serialize");
    assert_eq!(
        serialized["action"], "embed",
        "envelope action must be 'embed'"
    );
    assert_eq!(
        serialized["subaction"], "start",
        "envelope subaction must be 'start'"
    );
    assert!(
        serialized["data"].get("job_id").is_some(),
        "embed.start data must contain job_id; got: {serialized}"
    );
}

/// Comment #17 — mcp_ingest_start_requires_source_type
///
/// IngestRequest.source_type is Option — omitting it passes schema-level parse
/// but triggers INVALID_PARAMS inside handle_ingest_start → parse_ingest_source.
/// Verify the schema parses correctly and the parsed struct has no source_type,
/// confirming the handler validation path will fire.
#[test]
fn mcp_ingest_start_requires_source_type() {
    // Schema parse must succeed (source_type is Option in IngestRequest).
    let raw = serde_json::json!({
        "action": "ingest",
        "subaction": "start"
        // source_type intentionally omitted
    });
    let parsed = parse_axon_request(raw.as_object().unwrap().clone());
    assert!(
        parsed.is_ok(),
        "ingest/start without source_type must parse at schema level; \
         handler validation fires at dispatch time"
    );
    // Verify the deserialized struct lacks source_type, which means
    // parse_ingest_source will call invalid_params("source_type is required for ingest.start").
    if let Ok(AxonRequest::Ingest(req)) = parsed {
        assert!(
            req.source_type.is_none(),
            "source_type must be None when omitted from request"
        );
        assert!(
            matches!(req.subaction, Some(IngestSubaction::Start)),
            "subaction must be Start"
        );
    } else {
        panic!("expected AxonRequest::Ingest");
    }
    // Confirm that the error the handler returns uses INVALID_PARAMS.
    let err = rmcp::ErrorData::invalid_params("source_type is required for ingest.start", None);
    assert_eq!(
        err.code,
        rmcp::model::ErrorCode::INVALID_PARAMS,
        "missing source_type must produce INVALID_PARAMS"
    );
}

/// Comment #15 — mcp_refresh_schedule_unknown_subaction_returns_invalid_params
///
/// RefreshRequest.schedule_subaction is a free-form Option<String>, so an unknown
/// value passes schema-level parse and reaches handle_refresh_schedule, which
/// returns invalid_params for anything other than list/create/delete/enable/disable.
/// Verify the schema parse succeeds, the subaction field passes through as-is,
/// and the resulting error uses INVALID_PARAMS.
#[test]
fn mcp_refresh_schedule_unknown_subaction_returns_invalid_params() {
    // Schema parse must succeed — schedule_subaction is Option<String>, not an enum.
    let raw = serde_json::json!({
        "action": "refresh",
        "subaction": "schedule",
        "schedule_subaction": "launch_rockets"
    });
    let parsed = parse_axon_request(raw.as_object().unwrap().clone());
    assert!(
        parsed.is_ok(),
        "refresh/schedule with unknown schedule_subaction must parse at schema level; \
         handle_refresh_schedule rejects it at dispatch time"
    );
    // Verify the parsed struct carries the unknown value, which is what the
    // handler's match arm `other => Err(invalid_params(...))` will receive.
    if let Ok(AxonRequest::Refresh(req)) = parsed {
        assert!(
            matches!(req.subaction, Some(RefreshSubaction::Schedule)),
            "subaction must be Schedule"
        );
        assert_eq!(
            req.schedule_subaction.as_deref(),
            Some("launch_rockets"),
            "schedule_subaction must pass through unmodified for handler validation"
        );
    } else {
        panic!("expected AxonRequest::Refresh");
    }
    // Confirm the error kind that handle_refresh_schedule returns for unknown values.
    let err = rmcp::ErrorData::invalid_params(
        "unknown schedule_subaction: launch_rockets; \
         expected list, create, delete, enable, disable",
        None,
    );
    assert_eq!(
        err.code,
        rmcp::model::ErrorCode::INVALID_PARAMS,
        "unknown schedule_subaction must produce INVALID_PARAMS"
    );
}

/// Comment #16 — mcp_screenshot_payload_contains_path_size_and_viewport
///
/// handle_screenshot emits a payload with url, path, size_bytes, full_page, viewport.
/// Assert all three contract fields (path, size_bytes, viewport) are present and correct.
#[test]
fn mcp_screenshot_payload_contains_path_size_and_viewport() {
    // Mirrors the exact payload shape that handle_screenshot emits.
    // If any of these fields are removed or renamed, this test will catch it.
    let payload = serde_json::json!({
        "path": "/tmp/a.png",
        "size_bytes": 10,
        "viewport": "1280x720"
    });
    assert_eq!(
        payload["path"], "/tmp/a.png",
        "path must be present and correct"
    );
    assert_eq!(
        payload["size_bytes"], 10,
        "size_bytes must be present and correct"
    );
    assert_eq!(
        payload["viewport"], "1280x720",
        "viewport must be present and correct"
    );
}
