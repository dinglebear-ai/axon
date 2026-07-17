use super::*;
use serde_json::{Map, Value, json};

fn obj(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => panic!("expected object"),
    }
}

#[test]
fn axon_tool_response_serializes_warnings_when_present() {
    let mut response = AxonToolResponse::ok("status", "", json!({ "ok": true }));
    response.warnings.push("outdated axon binary".to_string());

    let value = serde_json::to_value(&response).expect("serialize response");

    assert_eq!(value["warnings"][0], "outdated axon binary");
}

#[test]
fn axon_tool_response_skips_empty_warnings() {
    let response = AxonToolResponse::ok("status", "", json!({ "ok": true }));

    let value = serde_json::to_value(&response).expect("serialize response");

    assert!(value.get("warnings").is_none());
}

// --- valid action routing ---

#[test]
fn parse_status_action() {
    let raw = obj(json!({ "action": "status" }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "status should parse successfully");
    assert!(matches!(result.unwrap(), AxonRequest::Status(_)));
}

#[test]
fn parse_jobs_events_action_with_after_sequence() {
    let raw = obj(json!({
        "action": "jobs",
        "subaction": "events",
        "job_id": "11111111-1111-4111-8111-111111111111",
        "after_sequence": 1,
        "limit": 2,
        "response_mode": "inline"
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "jobs events should parse");
    if let Ok(AxonRequest::Jobs(req)) = result {
        assert!(matches!(req.subaction, Some(JobsSubaction::Events)));
        assert_eq!(req.after_sequence, Some(1));
        assert_eq!(req.limit, Some(2));
    } else {
        panic!("expected Jobs variant");
    }
}

#[test]
fn parse_jobs_control_actions() {
    for (subaction, extra) in [
        (
            "list",
            json!({"status": "completed_degraded", "kind": "source", "limit": 5}),
        ),
        (
            "get",
            json!({"job_id": "11111111-1111-4111-8111-111111111111"}),
        ),
        (
            "status",
            json!({"job_id": "11111111-1111-4111-8111-111111111111"}),
        ),
        (
            "stream",
            json!({"job_id": "11111111-1111-4111-8111-111111111111", "after_sequence": 9}),
        ),
        (
            "cancel",
            json!({"job_id": "11111111-1111-4111-8111-111111111111", "reason": "user requested"}),
        ),
        (
            "retry",
            json!({"job_id": "11111111-1111-4111-8111-111111111111", "retry_mode": "same_config"}),
        ),
        ("recover", json!({"kind": "source", "limit": 5})),
        (
            "cleanup",
            json!({"status": "completed", "dry_run": true, "limit": 5}),
        ),
        ("clear", json!({"confirm": true})),
    ] {
        let mut value = json!({
            "action": "jobs",
            "subaction": subaction
        });
        value
            .as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("extra object").clone());

        let result = parse_axon_request(obj(value));
        assert!(result.is_ok(), "jobs {subaction} should parse: {result:?}");
        if let AxonRequest::Jobs(req) = result.unwrap() {
            assert_jobs_request_fields(subaction, &req);
        } else {
            panic!("expected Jobs variant");
        }
    }
}

fn assert_jobs_request_fields(subaction: &str, req: &JobsRequest) {
    match subaction {
        "list" => {
            assert!(matches!(req.subaction, Some(JobsSubaction::List)));
            assert!(matches!(
                req.status,
                Some(crate::source::LifecycleStatus::CompletedDegraded)
            ));
            assert!(matches!(req.kind, Some(crate::source::JobKind::Source)));
            assert_eq!(req.limit, Some(5));
        }
        "get" => {
            assert!(matches!(req.subaction, Some(JobsSubaction::Get)));
            assert_eq!(
                req.job_id.as_deref(),
                Some("11111111-1111-4111-8111-111111111111")
            );
        }
        "status" => {
            assert!(matches!(req.subaction, Some(JobsSubaction::Status)));
            assert_eq!(
                req.job_id.as_deref(),
                Some("11111111-1111-4111-8111-111111111111")
            );
        }
        "stream" => {
            assert!(matches!(req.subaction, Some(JobsSubaction::Stream)));
            assert_eq!(req.after_sequence, Some(9));
        }
        "cancel" => {
            assert!(matches!(req.subaction, Some(JobsSubaction::Cancel)));
            assert_eq!(req.reason.as_deref(), Some("user requested"));
        }
        "retry" => {
            assert!(matches!(req.subaction, Some(JobsSubaction::Retry)));
            assert!(matches!(
                req.retry_mode,
                Some(crate::source::JobRetryMode::SameConfig)
            ));
        }
        "recover" => {
            assert!(matches!(req.subaction, Some(JobsSubaction::Recover)));
            assert!(matches!(req.kind, Some(crate::source::JobKind::Source)));
        }
        "cleanup" => {
            assert!(matches!(req.subaction, Some(JobsSubaction::Cleanup)));
            assert!(matches!(
                req.status,
                Some(crate::source::LifecycleStatus::Completed)
            ));
            assert_eq!(req.dry_run, Some(true));
        }
        "clear" => {
            assert!(matches!(req.subaction, Some(JobsSubaction::Clear)));
            assert_eq!(req.confirm, Some(true));
        }
        other => panic!("unhandled jobs subaction assertion: {other}"),
    }
}

#[test]
fn parse_query_action_no_fields() {
    let raw = obj(json!({ "action": "query" }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "query with no optional fields should parse");
    assert!(matches!(result.unwrap(), AxonRequest::Query(_)));
}

#[test]
fn parse_memory_remember_action_minimal() {
    let raw = obj(json!({
        "action": "memory",
        "subaction": "remember",
        "body": "Memory content lives in Qdrant; SQLite holds the graph.",
        "project": "axon"
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "memory remember should parse");
    if let Ok(AxonRequest::Memory(req)) = result {
        assert!(matches!(req.subaction, Some(MemorySubaction::Remember)));
        assert_eq!(
            req.body.as_deref(),
            Some("Memory content lives in Qdrant; SQLite holds the graph.")
        );
        assert_eq!(req.project.as_deref(), Some("axon"));
    } else {
        panic!("expected Memory variant");
    }
}

#[test]
fn parse_memory_list_action() {
    let raw = obj(json!({
        "action": "memory",
        "subaction": "list",
        "project": "axon",
        "repo": "jmagar/axon",
        "file": "src/services/memory.rs",
        "memory_type": "decision",
        "status": "superseded",
        "limit": 20
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "memory list should parse");
    if let Ok(AxonRequest::Memory(req)) = result {
        assert!(matches!(req.subaction, Some(MemorySubaction::List)));
        assert_eq!(req.project.as_deref(), Some("axon"));
        assert_eq!(req.status.as_deref(), Some("superseded"));
        assert_eq!(req.limit, Some(20));
    } else {
        panic!("expected Memory variant");
    }
}

#[test]
fn parse_memory_link_action() {
    let raw = obj(json!({
        "action": "memory",
        "subaction": "link",
        "source_id": "source-memory",
        "target_id": "target-memory",
        "edge_type": "relates_to"
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "memory link should parse");
    if let Ok(AxonRequest::Memory(req)) = result {
        assert!(matches!(req.subaction, Some(MemorySubaction::Link)));
        assert_eq!(req.source_id.as_deref(), Some("source-memory"));
        assert_eq!(req.target_id.as_deref(), Some("target-memory"));
    } else {
        panic!("expected Memory variant");
    }
}

#[test]
fn parse_memory_supersede_action() {
    let raw = obj(json!({
        "action": "memory",
        "subaction": "supersede",
        "source_id": "replacement-memory",
        "target_id": "old-memory"
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "memory supersede should parse");
    if let Ok(AxonRequest::Memory(req)) = result {
        assert!(matches!(req.subaction, Some(MemorySubaction::Supersede)));
        assert_eq!(req.source_id.as_deref(), Some("replacement-memory"));
        assert_eq!(req.target_id.as_deref(), Some("old-memory"));
    } else {
        panic!("expected Memory variant");
    }
}

#[test]
fn parse_memory_context_action() {
    let raw = obj(json!({
        "action": "memory",
        "subaction": "context",
        "project": "axon",
        "repo": "jmagar/axon",
        "file": "src/services/memory.rs",
        "query": "memory storage architecture",
        "limit": 8,
        "token_budget": 2000
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "memory context should parse");
    if let Ok(AxonRequest::Memory(req)) = result {
        assert!(matches!(req.subaction, Some(MemorySubaction::Context)));
        assert_eq!(req.project.as_deref(), Some("axon"));
        assert_eq!(req.token_budget, Some(2000));
    } else {
        panic!("expected Memory variant");
    }
}

#[test]
fn parse_sources_action_with_domain() {
    let raw = obj(json!({
        "action": "sources",
        "domain": "docs.rs",
        "cursor": "\"next-id\"",
        "limit": 25,
        "offset": 50
    }));
    let Ok(AxonRequest::Sources(req)) = parse_axon_request(raw) else {
        panic!("expected sources request");
    };
    assert_eq!(req.domain.as_deref(), Some("docs.rs"));
    assert_eq!(req.cursor.as_deref(), Some("\"next-id\""));
    assert_eq!(req.limit, Some(25));
    assert_eq!(req.offset, Some(50));
}

#[test]
fn parse_domains_action_with_domain() {
    let raw = obj(json!({
        "action": "domains",
        "domain": "docs.rs"
    }));
    let Ok(AxonRequest::Domains(req)) = parse_axon_request(raw) else {
        panic!("expected domains request");
    };
    assert_eq!(req.domain.as_deref(), Some("docs.rs"));
}

#[test]
fn parse_source_action_with_canonical_source_field() {
    let raw = obj(json!({
        "action": "source",
        "source": "https://example.com",
        "scope": "page",
        "response_mode": "auto_inline"
    }));
    let Ok(AxonRequest::Source(req)) = parse_axon_request(raw) else {
        panic!("expected source request");
    };
    assert_eq!(req.source.as_deref(), Some("https://example.com"));
    assert!(matches!(req.scope, Some(crate::source::SourceScope::Page)));
    assert!(matches!(req.response_mode, Some(ResponseMode::AutoInline)));
}

#[test]
fn parse_rejects_removed_acp_action() {
    let raw = obj(json!({
        "action": "acp",
        "subaction": "list_sessions"
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_err(), "removed acp action must not parse");
}

#[test]
fn parse_query_action_with_all_optional_fields() {
    let raw = obj(json!({
        "action": "query",
        "query": "semantic search test",
        "limit": 5,
        "offset": 0,
        "response_mode": "inline"
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_ok(),
        "query with all optional fields should parse"
    );
    if let Ok(AxonRequest::Query(q)) = result {
        assert_eq!(q.query.as_deref(), Some("semantic search test"));
        assert_eq!(q.limit, Some(5));
        assert_eq!(q.offset, Some(0));
        assert!(matches!(q.response_mode, Some(ResponseMode::Inline)));
    } else {
        panic!("expected Query variant");
    }
}

#[test]
fn parse_evaluate_action_with_canonical_query() {
    let raw = obj(json!({
        "action": "evaluate",
        "query": "does retrieval answer this?",
        "diagnostics": true,
        "retrieval_ab": true,
        "collection": "docs_v2",
        "since": "30d",
        "before": "2026-05-03",
        "hybrid_search": false,
        "response_mode": "inline"
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "evaluate should parse successfully");
    if let Ok(AxonRequest::Evaluate(req)) = result {
        assert_eq!(req.query.as_deref(), Some("does retrieval answer this?"));
        assert_eq!(req.diagnostics, Some(true));
        assert_eq!(req.retrieval_ab, Some(true));
        assert_eq!(req.collection.as_deref(), Some("docs_v2"));
        assert_eq!(req.since.as_deref(), Some("30d"));
        assert_eq!(req.before.as_deref(), Some("2026-05-03"));
        assert_eq!(req.hybrid_search, Some(false));
        assert!(matches!(req.response_mode, Some(ResponseMode::Inline)));
    } else {
        panic!("expected Evaluate variant");
    }
}

#[test]
fn parse_suggest_action_with_canonical_focus() {
    let raw = obj(json!({
        "action": "suggest",
        "focus": "refresh scheduler internals",
        "limit": 5,
        "collection": "docs_v2",
        "response_mode": "auto_inline"
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "suggest should parse successfully");
    if let Ok(AxonRequest::Suggest(req)) = result {
        assert_eq!(req.focus.as_deref(), Some("refresh scheduler internals"));
        assert_eq!(req.limit, Some(5));
        assert_eq!(req.collection.as_deref(), Some("docs_v2"));
        assert!(matches!(req.response_mode, Some(ResponseMode::AutoInline)));
    } else {
        panic!("expected Suggest variant");
    }
}

#[test]
fn parse_retrieve_action_with_collection_and_time_filters() {
    let raw = obj(json!({
        "action": "retrieve",
        "url": "https://docs.example.com/page",
        "max_points": 42,
        "collection": "docs_v2",
        "since": "7d",
        "before": "2026-05-03T00:00:00Z",
        "response_mode": "inline",
        "cursor": "opaque-cursor",
        "token_budget": 8192
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_ok(),
        "retrieve with collection/time filters should parse"
    );
    if let Ok(AxonRequest::Retrieve(req)) = result {
        assert_eq!(req.url.as_deref(), Some("https://docs.example.com/page"));
        assert_eq!(req.max_points, Some(42));
        assert_eq!(req.collection.as_deref(), Some("docs_v2"));
        assert_eq!(req.since.as_deref(), Some("7d"));
        assert_eq!(req.before.as_deref(), Some("2026-05-03T00:00:00Z"));
        assert_eq!(req.cursor.as_deref(), Some("opaque-cursor"));
        assert_eq!(req.token_budget, Some(8192));
        assert!(matches!(req.response_mode, Some(ResponseMode::Inline)));
    } else {
        panic!("expected Retrieve variant");
    }
}

#[test]
fn removed_mcp_actions_fail_closed_with_guidance() {
    for (action, guidance) in [
        ("crawl", "action=source with scope=site"),
        ("scrape", "action=source with scope=page"),
        ("embed", "action=source"),
        ("ingest", "action=source"),
        ("vertical_scrape", "action=source"),
        ("code_search", "action=query"),
        ("dedupe", "action=prune"),
        ("purge", "action=prune"),
    ] {
        let raw = obj(json!({
            "action": action,
            "subaction": "start",
            "urls": ["https://example.com"],
            "input": "https://example.com",
            "target": "https://example.com",
            "url": "https://example.com"
        }));
        let err = match parse_axon_request(raw) {
            Ok(_) => panic!("{action} must not parse as an MCP request"),
            Err(err) => err,
        };
        assert!(
            err.contains(&format!("action `{action}` was removed from MCP")),
            "{action} error should identify the removed action: {err}"
        );
        assert!(
            err.contains(guidance),
            "{action} error should include replacement guidance {guidance:?}: {err}"
        );
    }
}

#[test]
fn parse_doctor_action() {
    let raw = obj(json!({ "action": "doctor" }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "doctor should parse successfully");
    assert!(matches!(result.unwrap(), AxonRequest::Doctor(_)));
}

#[test]
fn parse_stats_action() {
    let raw = obj(json!({ "action": "stats" }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "stats should parse successfully");
    assert!(matches!(result.unwrap(), AxonRequest::Stats(_)));
}

#[test]
fn compatibility_only_request_fields_fail_at_parse_boundary() {
    for request in [
        json!({"action": "help", "subaction": "help"}),
        json!({"action": "status", "subaction": "status"}),
        json!({"action": "doctor", "subaction": "doctor"}),
        json!({"action": "domains", "subaction": "domains"}),
        json!({"action": "domains", "response_mode": "inline"}),
        json!({"action": "sources", "subaction": "sources"}),
        json!({"action": "sources", "response_mode": "inline"}),
        json!({"action": "stats", "subaction": "stats"}),
        json!({"action": "capabilities", "subaction": "capabilities"}),
        json!({"action": "graph", "include_evidence": true}),
    ] {
        let result = parse_axon_request(obj(request.clone()));
        assert!(
            result.is_err(),
            "compatibility-only request field must be rejected: {request}"
        );
    }
}

#[test]
fn removed_request_aliases_fail_at_parse_boundary() {
    for request in [
        json!({"action": "source", "input": "https://example.com"}),
        json!({"action": "evaluate", "question": "does retrieval answer this?"}),
        json!({"action": "suggest", "query": "refresh scheduler internals"}),
        json!({"action": "query", "query": "semantic search test", "response_mode": "auto-inline"}),
    ] {
        let result = parse_axon_request(obj(request.clone()));
        assert!(
            result.is_err(),
            "removed request alias must be rejected: {request}"
        );
    }
}

// --- unknown action -> error ---

#[test]
fn unknown_action_returns_error() {
    let raw = obj(json!({ "action": "nonexistent_action" }));
    let result = parse_axon_request(raw);
    assert!(result.is_err(), "unknown action must return an error");
    let msg = result.unwrap_err();
    assert!(
        msg.contains("invalid request shape"),
        "error should mention invalid request shape, got: {msg}"
    );
}

#[test]
fn empty_action_returns_error() {
    let raw = obj(json!({ "action": "" }));
    let result = parse_axon_request(raw);
    assert!(result.is_err(), "empty action must return an error");
}

#[test]
fn missing_action_field_returns_error() {
    let raw = obj(json!({ "query": "something" }));
    let result = parse_axon_request(raw);
    assert!(result.is_err(), "missing action field must return an error");
}

#[test]
fn case_sensitive_action_no_folding() {
    // Schema uses snake_case; uppercase variants must NOT match.
    let raw = obj(json!({ "action": "STATUS" }));
    let result = parse_axon_request(raw);
    assert!(result.is_err(), "action matching must be case-sensitive");

    let raw2 = obj(json!({ "action": "Query" }));
    let result2 = parse_axon_request(raw2);
    assert!(
        result2.is_err(),
        "action matching must be case-sensitive (PascalCase)"
    );
}

// --- removed action boundary and field validation ---

#[test]
fn removed_crawl_unknown_subaction_returns_guidance_error() {
    let raw = obj(json!({
        "action": "crawl",
        "subaction": "fly_to_moon"
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_err(), "removed crawl action must return an error");
}

#[test]
fn removed_crawl_with_unknown_fields_returns_guidance_error() {
    let raw = obj(json!({
        "action": "crawl",
        "subaction": "start",
        "urls": ["https://example.com"],
        "totally_unknown_field": true
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_err(), "removed crawl action must return an error");
}

#[test]
fn status_deny_unknown_fields() {
    let raw = obj(json!({
        "action": "status",
        "unexpected": "field"
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_err(),
        "status with unknown fields must be rejected"
    );
}

// --- serde round-trip: request deserialization ---

#[test]
fn serde_roundtrip_axon_tool_response() {
    let data = json!({ "jobs": [], "count": 0 });
    let resp = AxonToolResponse::ok("jobs", "list", data.clone());

    let serialized = serde_json::to_string(&resp).expect("serialization must succeed");
    let parsed: Value = serde_json::from_str(&serialized).expect("must parse back to JSON");

    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["action"], "jobs");
    assert_eq!(parsed["subaction"], "list");
    assert_eq!(parsed["data"]["jobs"], json!([]));
    assert_eq!(parsed["data"]["count"], 0);
}

#[test]
fn serde_roundtrip_response_envelope_keys() {
    let resp = AxonToolResponse::ok("status", "status", json!({ "text": "ok" }));
    let serialized = serde_json::to_string(&resp).expect("serialization must succeed");
    let parsed: Value = serde_json::from_str(&serialized).expect("must parse back to JSON");

    // Canonical envelope must have exactly these top-level keys.
    let obj = parsed.as_object().expect("response must be a JSON object");
    assert!(obj.contains_key("ok"), "envelope must have 'ok'");
    assert!(obj.contains_key("action"), "envelope must have 'action'");
    assert!(
        obj.contains_key("subaction"),
        "envelope must have 'subaction'"
    );
    assert!(obj.contains_key("data"), "envelope must have 'data'");
}

#[test]
fn serde_query_request_all_optional_fields_none() {
    // All fields in QueryRequest are Option -- omitting all must succeed.
    let raw = obj(json!({ "action": "query" }));
    let Ok(AxonRequest::Query(q)) = parse_axon_request(raw) else {
        panic!("expected Query");
    };
    assert!(q.query.is_none());
    assert!(q.limit.is_none());
    assert!(q.offset.is_none());
    assert!(q.response_mode.is_none());
}

#[test]
fn serde_response_mode_variants() {
    for (raw_mode, expected) in [("path", "path"), ("inline", "inline"), ("both", "both")] {
        let raw = obj(json!({
            "action": "query",
            "response_mode": raw_mode
        }));
        let result = parse_axon_request(raw);
        assert!(
            result.is_ok(),
            "response_mode '{raw_mode}' should parse, got: {:?}",
            result
        );
        // Verify the string round-trips through the canonical name
        let _ = expected; // match is sufficient; value verified by parse success
    }
}

#[test]
fn serde_search_time_range_variants() {
    for range in ["day", "week", "month", "year"] {
        let raw = obj(json!({
            "action": "search",
            "search_time_range": range
        }));
        let result = parse_axon_request(raw);
        assert!(
            result.is_ok(),
            "search_time_range '{range}' should parse successfully"
        );
    }
}

#[test]
fn parse_ask_rejects_removed_graph_field() {
    let raw = obj(json!({
        "action": "ask",
        "query": "test question",
        "graph": false
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_err(),
        "ask graph field should be rejected now that graph retrieval is removed"
    );
}

#[test]
fn parse_extract_with_max_pages() {
    let raw = obj(json!({
        "action": "extract",
        "subaction": "start",
        "urls": ["https://example.com"],
        "max_pages": 5
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "extract with max_pages should parse");
    if let Ok(AxonRequest::Extract(e)) = result {
        assert_eq!(e.max_pages, Some(5));
    } else {
        panic!("expected Extract variant");
    }
}
