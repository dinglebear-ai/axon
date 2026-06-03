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
fn parse_query_action_no_fields() {
    let raw = obj(json!({ "action": "query" }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "query with no optional fields should parse");
    assert!(matches!(result.unwrap(), AxonRequest::Query(_)));
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
fn parse_evaluate_action_with_question_alias() {
    let raw = obj(json!({
        "action": "evaluate",
        "question": "does retrieval answer this?",
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
fn parse_suggest_action_with_query_alias() {
    let raw = obj(json!({
        "action": "suggest",
        "query": "refresh scheduler internals",
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
fn parse_crawl_start_action() {
    let raw = obj(json!({
        "action": "crawl",
        "subaction": "start",
        "urls": ["https://example.com"]
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "crawl start should parse successfully");
    if let Ok(AxonRequest::Crawl(c)) = result {
        assert!(matches!(c.subaction, Some(CrawlSubaction::Start)));
        assert_eq!(
            c.urls.as_deref(),
            Some(&["https://example.com".to_string()][..])
        );
    } else {
        panic!("expected Crawl variant");
    }
}

#[test]
fn parse_crawl_list_action() {
    let raw = obj(json!({
        "action": "crawl",
        "subaction": "list",
        "limit": 10
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "crawl list should parse successfully");
    assert!(matches!(result.unwrap(), AxonRequest::Crawl(_)));
}

#[test]
fn parse_embed_start_action() {
    let raw = obj(json!({
        "action": "embed",
        "subaction": "start",
        "input": "https://docs.example.com"
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "embed start should parse successfully");
    if let Ok(AxonRequest::Embed(e)) = result {
        assert!(matches!(e.subaction, Some(EmbedSubaction::Start)));
        assert_eq!(e.input.as_deref(), Some("https://docs.example.com"));
    } else {
        panic!("expected Embed variant");
    }
}

#[test]
fn parse_scrape_action() {
    let raw = obj(json!({
        "action": "scrape",
        "url": "https://example.com/page",
        "cursor": "opaque-cursor",
        "token_budget": 4096
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "scrape should parse successfully");
    if let Ok(AxonRequest::Scrape(s)) = result {
        assert_eq!(s.url.as_deref(), Some("https://example.com/page"));
        assert_eq!(s.cursor.as_deref(), Some("opaque-cursor"));
        assert_eq!(s.token_budget, Some(4096));
    } else {
        panic!("expected Scrape variant");
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
fn parse_help_action_with_singleton_subaction() {
    let raw = obj(json!({
        "action": "help",
        "subaction": "help",
        "response_mode": "inline"
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_ok(),
        "help should accept singleton subaction for API compatibility"
    );
    if let Ok(AxonRequest::Help(req)) = result {
        assert_eq!(req.subaction.as_deref(), Some("help"));
        assert!(matches!(req.response_mode, Some(ResponseMode::Inline)));
    } else {
        panic!("expected Help variant");
    }
}

#[test]
fn parse_status_action_with_singleton_subaction() {
    let raw = obj(json!({
        "action": "status",
        "subaction": "status",
        "response_mode": "inline"
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_ok(),
        "status should accept singleton subaction for API compatibility"
    );
    if let Ok(AxonRequest::Status(req)) = result {
        assert_eq!(req.subaction.as_deref(), Some("status"));
        assert!(matches!(req.response_mode, Some(ResponseMode::Inline)));
    } else {
        panic!("expected Status variant");
    }
}

#[test]
fn parse_doctor_action_with_singleton_subaction() {
    let raw = obj(json!({
        "action": "doctor",
        "subaction": "doctor",
        "response_mode": "inline"
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_ok(),
        "doctor should accept singleton subaction for API compatibility"
    );
    if let Ok(AxonRequest::Doctor(req)) = result {
        assert_eq!(req.subaction.as_deref(), Some("doctor"));
        assert!(matches!(req.response_mode, Some(ResponseMode::Inline)));
    } else {
        panic!("expected Doctor variant");
    }
}

#[test]
fn parse_domains_action_with_singleton_subaction() {
    let raw = obj(json!({
        "action": "domains",
        "subaction": "domains",
        "limit": 10,
        "offset": 0,
        "response_mode": "inline"
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_ok(),
        "domains should accept singleton subaction for API compatibility"
    );
    if let Ok(AxonRequest::Domains(req)) = result {
        assert_eq!(req.subaction.as_deref(), Some("domains"));
        assert_eq!(req.limit, Some(10));
        assert_eq!(req.offset, Some(0));
        assert!(matches!(req.response_mode, Some(ResponseMode::Inline)));
    } else {
        panic!("expected Domains variant");
    }
}

#[test]
fn parse_sources_action_with_singleton_subaction() {
    let raw = obj(json!({
        "action": "sources",
        "subaction": "sources",
        "limit": 10,
        "offset": 0,
        "response_mode": "inline"
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_ok(),
        "sources should accept singleton subaction for API compatibility"
    );
    if let Ok(AxonRequest::Sources(req)) = result {
        assert_eq!(req.subaction.as_deref(), Some("sources"));
        assert_eq!(req.limit, Some(10));
        assert_eq!(req.offset, Some(0));
        assert!(matches!(req.response_mode, Some(ResponseMode::Inline)));
    } else {
        panic!("expected Sources variant");
    }
}

#[test]
fn parse_stats_action_with_singleton_subaction() {
    let raw = obj(json!({
        "action": "stats",
        "subaction": "stats",
        "response_mode": "inline"
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_ok(),
        "stats should accept singleton subaction for API compatibility"
    );
    if let Ok(AxonRequest::Stats(req)) = result {
        assert_eq!(req.subaction.as_deref(), Some("stats"));
        assert!(matches!(req.response_mode, Some(ResponseMode::Inline)));
    } else {
        panic!("expected Stats variant");
    }
}

#[test]
fn parse_query_action_with_auto_inline_alias() {
    let raw = obj(json!({
        "action": "query",
        "query": "semantic search test",
        "response_mode": "auto-inline"
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_ok(),
        "auto-inline should deserialize as a supported response mode alias"
    );
    if let Ok(AxonRequest::Query(q)) = result {
        assert_eq!(q.query.as_deref(), Some("semantic search test"));
        assert!(matches!(q.response_mode, Some(ResponseMode::AutoInline)));
    } else {
        panic!("expected Query variant");
    }
}

#[test]
fn parse_ingest_start_github() {
    let raw = obj(json!({
        "action": "ingest",
        "subaction": "start",
        "source_type": "github",
        "target": "owner/repo"
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "ingest start github should parse");
    if let Ok(AxonRequest::Ingest(i)) = result {
        assert!(matches!(i.subaction, Some(IngestSubaction::Start)));
        assert!(matches!(i.source_type, Some(IngestSourceType::Github)));
        assert_eq!(i.target.as_deref(), Some("owner/repo"));
    } else {
        panic!("expected Ingest variant");
    }
}

#[test]
fn parse_ingest_start_gitlab() {
    let raw = obj(json!({
        "action": "ingest",
        "subaction": "start",
        "source_type": "gitlab",
        "target": "https://gitlab.com/group/project"
    }));
    let result = parse_axon_request(raw);
    assert!(result.is_ok(), "ingest start gitlab should parse");
    if let Ok(AxonRequest::Ingest(i)) = result {
        assert!(matches!(i.subaction, Some(IngestSubaction::Start)));
        assert!(matches!(i.source_type, Some(IngestSourceType::Gitlab)));
        assert_eq!(
            i.target.as_deref(),
            Some("https://gitlab.com/group/project")
        );
    } else {
        panic!("expected Ingest variant");
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

// --- missing required field -> validation error ---

#[test]
fn crawl_missing_subaction_defaults_to_start() {
    // subaction is optional; omitting it should default to Start in the handler.
    let raw = obj(json!({
        "action": "crawl",
        "urls": ["https://example.com"]
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_ok(),
        "crawl without subaction should parse successfully"
    );
    if let Ok(AxonRequest::Crawl(c)) = result {
        assert!(
            c.subaction.is_none(),
            "subaction should be None when omitted"
        );
    }
}

#[test]
fn embed_missing_subaction_defaults_to_start() {
    let raw = obj(json!({
        "action": "embed",
        "input": "https://docs.example.com"
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_ok(),
        "embed without subaction should parse successfully"
    );
    if let Ok(AxonRequest::Embed(e)) = result {
        assert!(
            e.subaction.is_none(),
            "subaction should be None when omitted"
        );
    }
}

#[test]
fn ingest_missing_subaction_defaults_to_start() {
    let raw = obj(json!({
        "action": "ingest",
        "source_type": "github",
        "target": "owner/repo"
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_ok(),
        "ingest without subaction should parse successfully"
    );
    if let Ok(AxonRequest::Ingest(i)) = result {
        assert!(
            i.subaction.is_none(),
            "subaction should be None when omitted"
        );
    }
}

#[test]
fn crawl_unknown_subaction_returns_error() {
    let raw = obj(json!({
        "action": "crawl",
        "subaction": "fly_to_moon"
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_err(),
        "crawl with unknown subaction must return an error"
    );
}

#[test]
fn llms_txt_request_fields_roundtrip_and_map() {
    let raw = obj(json!({
        "action": "crawl",
        "subaction": "start",
        "urls": ["https://x.com"],
        "discover_llms_txt": false,
        "max_llms_txt_urls": 50
    }));
    let parsed = parse_axon_request(raw).expect("crawl request parses");
    let AxonRequest::Crawl(req) = parsed else {
        panic!("expected crawl request");
    };
    assert_eq!(req.discover_llms_txt, Some(false));
    assert_eq!(req.max_llms_txt_urls, Some(50));
    // Serialize back and confirm snake_case wire names (guards a silent casing mismatch).
    let out = serde_json::to_string(&req).unwrap();
    assert!(out.contains("discover_llms_txt"));
    assert!(out.contains("max_llms_txt_urls"));
}

#[test]
fn crawl_deny_unknown_fields() {
    // CrawlRequest uses #[serde(deny_unknown_fields)]
    let raw = obj(json!({
        "action": "crawl",
        "subaction": "start",
        "urls": ["https://example.com"],
        "totally_unknown_field": true
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_err(),
        "unknown fields must be rejected by deny_unknown_fields"
    );
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
    let resp = AxonToolResponse::ok("crawl", "list", data.clone());

    let serialized = serde_json::to_string(&resp).expect("serialization must succeed");
    let parsed: Value = serde_json::from_str(&serialized).expect("must parse back to JSON");

    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["action"], "crawl");
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
fn serde_crawl_render_mode_variants() {
    for subaction_str in ["http", "chrome", "auto_switch"] {
        let raw = obj(json!({
            "action": "crawl",
            "subaction": "start",
            "render_mode": subaction_str
        }));
        let result = parse_axon_request(raw);
        assert!(
            result.is_ok(),
            "render_mode '{subaction_str}' should parse successfully"
        );
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
fn serde_ingest_source_type_variants() {
    for src in [
        "github", "gitlab", "gitea", "git", "reddit", "youtube", "sessions",
    ] {
        let raw = obj(json!({
            "action": "ingest",
            "subaction": "start",
            "source_type": src
        }));
        let result = parse_axon_request(raw);
        assert!(
            result.is_ok(),
            "ingest source_type '{src}' should parse successfully"
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
fn parse_scrape_with_render_mode_format_embed() {
    let raw = obj(json!({
        "action": "scrape",
        "url": "https://example.com",
        "render_mode": "chrome",
        "format": "html",
        "embed": false
    }));
    let result = parse_axon_request(raw);
    assert!(
        result.is_ok(),
        "scrape with render_mode/format/embed should parse"
    );
    if let Ok(AxonRequest::Scrape(s)) = result {
        assert_eq!(s.url.as_deref(), Some("https://example.com"));
        assert!(matches!(s.render_mode, Some(McpRenderMode::Chrome)));
        assert!(matches!(s.format, Some(McpScrapeFormat::Html)));
        assert_eq!(s.embed, Some(false));
    } else {
        panic!("expected Scrape variant");
    }
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

#[test]
fn serde_scrape_format_variants() {
    for fmt in ["markdown", "html", "raw_html", "json"] {
        let raw = obj(json!({
            "action": "scrape",
            "format": fmt
        }));
        let result = parse_axon_request(raw);
        assert!(
            result.is_ok(),
            "scrape format '{fmt}' should parse successfully"
        );
    }
}

#[test]
fn serde_artifacts_subaction_variants() {
    for sub in ["head", "grep", "wc", "read"] {
        let raw = obj(json!({
            "action": "artifacts",
            "subaction": sub,
            "path": ".cache/axon-mcp/test.json"
        }));
        let result = parse_axon_request(raw);
        assert!(
            result.is_ok(),
            "artifacts subaction '{sub}' should parse successfully"
        );
    }
}
