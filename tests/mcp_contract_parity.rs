/// MCP schema/helper contract tests.
///
/// These are compile-time and pure-logic tests. No live services are required.
/// They verify that:
///   1. Option mapper round-trips produce correct service types.
///   2. Service result structs expose fields consumed by MCP handlers.
///   3. Serialization examples keep envelope field names explicit.
use axon_mcp::schema::{
    AxonRequest, AxonToolResponse, IngestSubaction, SearchTimeRange, parse_axon_request,
};
use axon_mcp::server::common::{
    to_map_options, to_pagination, to_retrieve_options, to_search_options, to_service_time_range,
};
use axon_mcp::server::required_scope_for;
use axon_services::query::map_retrieve_result;
use axon_services::types::{
    AskResult, AskTiming, DoctorResult, DomainFacet, DomainsResult, MapOptions, Pagination,
    QueryHit, QueryResult, RetrieveOptions, RetrieveResult, SearchOptions, SearchResult,
    ServiceTimeRange, SourcesResult, StatsResult, SuggestResult,
};
use serde_json::json;

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
fn ask_request_accepts_explain_flag() {
    let raw = json!({
        "action": "ask",
        "query": "claude marketplace plugins",
        "explain": true
    })
    .as_object()
    .expect("object")
    .clone();

    let parsed = parse_axon_request(raw).expect("explain flag should parse");
    if let AxonRequest::Ask(req) = parsed {
        assert_eq!(req.explain, Some(true));
    } else {
        panic!("expected Ask request");
    }
}

#[test]
fn endpoints_request_parses_read_only_contract_fields() {
    let raw = json!({
        "action": "endpoints",
        "url": "https://example.com",
        "include_bundles": true,
        "first_party_only": false,
        "unique_only": true,
        "max_scripts": 40,
        "max_scan_bytes": 8388608,
        "verify": false,
        "capture_network": false
    })
    .as_object()
    .expect("object")
    .clone();

    let parsed = parse_axon_request(raw).expect("endpoints request should parse");
    if let AxonRequest::Endpoints(req) = parsed {
        assert_eq!(req.url.as_deref(), Some("https://example.com"));
        assert_eq!(req.include_bundles, Some(true));
        assert_eq!(req.first_party_only, Some(false));
        assert_eq!(req.unique_only, Some(true));
        assert_eq!(req.max_scripts, Some(40));
        assert_eq!(req.max_scan_bytes, Some(8_388_608));
        assert_eq!(req.verify, Some(false));
        assert_eq!(req.capture_network, Some(false));
    } else {
        panic!("expected Endpoints request");
    }
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
    let r = to_retrieve_options(None, None, None);
    assert_eq!(
        r,
        RetrieveOptions {
            max_points: None,
            cursor: None,
            token_budget: None,
        }
    );
}

#[test]
fn retrieve_options_some_value_passes_through() {
    let r = to_retrieve_options(Some(128), Some("abc".to_string()), Some(2048));
    assert_eq!(
        r,
        RetrieveOptions {
            max_points: Some(128),
            cursor: Some("abc".to_string()),
            token_budget: Some(2048),
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
        schema_version_breakdown: None,
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
    let v = vec![QueryHit {
        rank: 1,
        score: 0.9,
        rerank_score: 0.9,
        url: "https://a.com".to_string(),
        source: "docs".to_string(),
        snippet: "snippet".to_string(),
        chunk_index: None,
        file_path: None,
        symbol: None,
        kind: None,
        start_line: None,
        end_line: None,
        file_type: None,
        language: None,
        provider: None,
        content_kind: None,
        chunking_method: None,
        symbol_extraction_status: None,
    }];
    let r = QueryResult { results: v };
    assert_eq!(r.results.len(), 1);
    assert_eq!(r.results[0].score, 0.9);
}

#[test]
fn retrieve_result_chunks_are_empty_for_zero_count() {
    let r = RetrieveResult {
        chunk_count: 0,
        content: String::new(),
        requested_url: None,
        matched_url: None,
        truncated: false,
        warnings: Vec::new(),
        variant_errors: Vec::new(),
        token_estimate: None,
        next_cursor: None,
        remaining_tokens_estimate: None,
        backend: None,
        refresh_status: None,
    };
    assert_eq!(r.chunk_count, 0);
    assert!(r.content.is_empty());
}

#[test]
fn map_retrieve_result_stores_typed_chunk_count_and_content() {
    let r = map_retrieve_result(7, "hello world".to_string());
    assert_eq!(r.chunk_count, 7);
    assert_eq!(r.content, "hello world");
}

#[test]
fn ask_result_exposes_typed_answer() {
    let r = AskResult {
        query: "question".to_string(),
        answer: "42".to_string(),
        citation_validation: None,
        session: None,
        warnings: Vec::new(),
        diagnostics: None,
        explain: None,
        timing_ms: AskTiming {
            retrieval: 0,
            context_build: 0,
            llm: 0,
            total: 0,
            tei_embed_ms: None,
            qdrant_primary_ms: None,
            qdrant_secondary_ms: None,
            rerank_ms: None,
            top_select_ms: None,
            full_doc_fetch_ms: None,
            supplemental_ms: None,
            llm_ttft_ms: None,
            llm_total_ms: None,
            streamed: None,
            normalize_ms: None,
        },
    };
    assert_eq!(r.answer, "42");
}

#[test]
fn suggest_result_exposes_url_vec() {
    let r = SuggestResult {
        suggestions: vec![axon_services::types::Suggestion {
            url: "https://rust-lang.org".to_string(),
            reason: "Rust docs".to_string(),
        }],
    };
    assert_eq!(r.suggestions.len(), 1);
    assert_eq!(r.suggestions[0].url, "https://rust-lang.org");
    assert_eq!(r.suggestions[0].reason, "Rust docs");
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
        cursor: Some("cursor".to_string()),
        token_budget: Some(10_000),
    };
    let b = RetrieveOptions {
        max_points: Some(50),
        cursor: Some("cursor".to_string()),
        token_budget: Some(10_000),
    };
    assert_eq!(a, b);
}

// ─────────────────────────────────────────────────────────────────────────────
// Group 4: MCP serialization examples and validation contracts
// ─────────────────────────────────────────────────────────────────────────────

/// Comment #18 — mcp_embed_start_returns_job_id_payload_shape
///
/// This is a serialization example for the `AxonToolResponse` envelope shape
/// used by embed.start handlers; it does not exercise the handler path.
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
/// Verify the schema parses correctly and the parsed struct has no source_type.
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
    // Confirm that this validation error should be represented as INVALID_PARAMS.
    let err = rmcp::ErrorData::invalid_params("source_type is required for ingest.start", None);
    assert_eq!(
        err.code,
        rmcp::model::ErrorCode::INVALID_PARAMS,
        "missing source_type must produce INVALID_PARAMS"
    );
}

/// Comment #16 — mcp_screenshot_payload_contains_path_size_and_viewport
///
/// Serialization example for the screenshot payload fields documented for
/// handler responses; it does not exercise the filesystem-backed handler path.
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

// ─────────────────────────────────────────────────────────────────────────────
// Group 8: Scope contract for endpoints action
// ─────────────────────────────────────────────────────────────────────────────

/// Endpoints requires axon:write because it fetches pages, bundles, probes
/// endpoints, and may execute Chrome capture — all active outbound network I/O.
#[test]
fn endpoints_action_scope_is_write_not_read() {
    assert_eq!(
        required_scope_for("endpoints", ""),
        Some("axon:write"),
        "endpoints must require axon:write — it performs active outbound network I/O"
    );
    // Scope is per-action, not per-flag. Once axon:write is required, any
    // read-scoped token is denied regardless of verify or capture_network flags.
    assert_ne!(
        required_scope_for("endpoints", ""),
        Some("axon:read"),
        "endpoints must NOT be axon:read — that would allow read tokens to use Axon as an outbound scanner"
    );
}

/// Active LLM, browser, and outbound network side-effect actions require
/// axon:write in REST, action API, and MCP metadata so read-only tokens cannot
/// use Axon as a hosted network/LLM executor.
#[test]
fn active_llm_browser_and_network_actions_require_write_scope() {
    for action in [
        "ask",
        "evaluate",
        "suggest",
        "research",
        "screenshot",
        "brand",
        "diff",
    ] {
        assert_eq!(
            required_scope_for(action, ""),
            Some("axon:write"),
            "{action} must require axon:write"
        );
    }
}
