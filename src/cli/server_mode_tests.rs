use super::*;
use crate::core::config::CommandKind;
use crate::core::config::RenderMode;
use crate::services::client_contract::{
    RestIngestRequest, RestRouteContract, RestSummarizeRequest, rest_route_contracts,
};
use crate::services::types::ServiceJob;
use chrono::Utc;
use serde_json::json;
use std::collections::BTreeSet;
use uuid::Uuid;

fn cfg(command: CommandKind, positional: &[&str]) -> Config {
    let mut cfg = Config::test_default();
    cfg.command = command;
    cfg.positional = positional.iter().map(|value| value.to_string()).collect();
    cfg.server_url = Some(reqwest::Url::parse("http://127.0.0.1:8001").unwrap());
    cfg
}

#[test]
fn client_server_dispatch_routes_stateful_commands_to_server_client() {
    for command in [
        CommandKind::Status,
        CommandKind::Map,
        CommandKind::Scrape,
        CommandKind::Search,
        CommandKind::Research,
        CommandKind::Crawl,
        CommandKind::Extract,
        CommandKind::Embed,
        CommandKind::Ingest,
        CommandKind::Sessions,
        CommandKind::Query,
        CommandKind::Retrieve,
        CommandKind::Sources,
        CommandKind::Domains,
        CommandKind::Stats,
        CommandKind::Doctor,
        CommandKind::Ask,
        CommandKind::Evaluate,
        CommandKind::Suggest,
    ] {
        let cfg = cfg(command, &["https://example.com"]);
        assert_eq!(
            client_server_dispatch(&cfg),
            ClientServerDispatch::Server,
            "{command:?} should use ServerClient"
        );
        assert!(
            render::server_human_renderer_available(command),
            "{command:?} server route needs an explicit human renderer"
        );
    }
}

#[test]
fn client_server_dispatch_keeps_screenshot_local_without_rest_endpoint() {
    let cfg = cfg(CommandKind::Screenshot, &["https://example.com"]);

    assert_eq!(client_server_dispatch(&cfg), ClientServerDispatch::Local);
}

#[test]
fn client_server_dispatch_explicit_local_mode_uses_local_paths() {
    let mut cfg = cfg(CommandKind::Crawl, &["https://example.com"]);
    cfg.local_mode = true;

    assert_eq!(client_server_dispatch(&cfg), ClientServerDispatch::Local);
}

#[test]
fn client_server_dispatch_keeps_status_watch_local() {
    let mut cfg = cfg(CommandKind::Status, &[]);
    cfg.watch_mode = true;

    assert_eq!(client_server_dispatch(&cfg), ClientServerDispatch::Local);
}

#[test]
fn scrape_server_mode_uses_rest_contract_body() {
    let mut cfg = cfg(CommandKind::Scrape, &["https://example.com"]);
    cfg.embed = true;
    cfg.render_mode = RenderMode::Chrome;

    let plan = plan::server_rest_plan(&cfg).expect("scrape plan");

    assert_eq!(plan.path, "/v1/scrape");
    assert_eq!(
        plan.body,
        json!({
            "url": "https://example.com",
            "urls": null,
            "render_mode": "chrome",
            "format": "markdown",
            "embed": true,
            "collection": "axon",
            "root_selector": null,
            "exclude_selector": null,
            "headers": [],
        })
    );
}

#[test]
fn scrape_server_mode_forwards_skip_embed() {
    let mut cfg = cfg(CommandKind::Scrape, &["https://example.com"]);
    cfg.embed = false;

    let plan = plan::server_rest_plan(&cfg).expect("scrape plan");

    assert_eq!(
        plan.body,
        json!({
            "url": "https://example.com",
            "urls": null,
            "render_mode": "auto-switch",
            "format": "markdown",
            "embed": false,
            "collection": "axon",
            "root_selector": null,
            "exclude_selector": null,
            "headers": [],
        })
    );
}

#[test]
fn server_mode_post_bodies_match_canonical_rest_contract_fields() {
    for contract in rest_route_contracts() {
        let actual = server_mode_body_fields(contract);
        let expected = contract
            .fields
            .iter()
            .map(|field| field.to_string())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            actual, expected,
            "server-mode planner body drifted from canonical REST contract for {} {}",
            contract.method, contract.path
        );
    }
}

fn server_mode_body_fields(contract: &RestRouteContract) -> BTreeSet<String> {
    let plans = match contract.path {
        "/v1/scrape" => vec![plan_for(CommandKind::Scrape, &["https://example.com"])],
        "/v1/summarize" => vec![plan_for(CommandKind::Summarize, &["https://example.com"])],
        "/v1/crawl" => vec![plan_for(CommandKind::Crawl, &["https://example.com"])],
        "/v1/extract" => vec![plan_for(CommandKind::Extract, &["https://example.com"])],
        "/v1/embed" => vec![plan_for(CommandKind::Embed, &["plain text"])],
        "/v1/ingest" => vec![
            plan_for(
                CommandKind::Ingest,
                &["https://github.com/MCPJam/inspector"],
            ),
            plan_for(CommandKind::Sessions, &[]),
        ],
        "/v1/ask" => vec![plan_with_query(CommandKind::Ask, "what is parity?")],
        "/v1/query" => vec![plan_with_query(CommandKind::Query, "parity query")],
        "/v1/retrieve" => vec![plan_for(CommandKind::Retrieve, &["https://example.com"])],
        "/v1/evaluate" => vec![plan_with_query(CommandKind::Evaluate, "is parity intact?")],
        "/v1/suggest" => vec![plan_with_query(CommandKind::Suggest, "rust docs")],
        "/v1/map" => vec![plan_for(CommandKind::Map, &["https://example.com"])],
        "/v1/search" => vec![plan_with_query(CommandKind::Search, "mcp rust")],
        "/v1/research" => vec![plan_with_query(CommandKind::Research, "mcp rust")],
        path => panic!("no server-mode sample for REST contract path {path}"),
    };

    plans
        .iter()
        .inspect(|plan| {
            assert_eq!(plan.method, contract.method);
            assert_eq!(plan.path, contract.path);
        })
        .flat_map(|plan| {
            plan.body
                .as_object()
                .unwrap_or_else(|| panic!("{} {} body is not an object", plan.method, plan.path))
                .keys()
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect()
}

fn plan_for(command: CommandKind, positional: &[&str]) -> plan::ServerRestPlan {
    let cfg = cfg(command, positional);
    plan::server_rest_plan(&cfg).unwrap_or_else(|err| panic!("{command:?} plan: {err}"))
}

fn plan_with_query(command: CommandKind, query: &str) -> plan::ServerRestPlan {
    let mut cfg = cfg(command, &[]);
    cfg.query = Some(query.to_string());
    plan::server_rest_plan(&cfg).unwrap_or_else(|err| panic!("{command:?} plan: {err}"))
}

#[test]
fn summarize_server_mode_forwards_scrape_controls() {
    let mut cfg = cfg(CommandKind::Summarize, &["https://example.com"]);
    cfg.render_mode = RenderMode::Chrome;
    cfg.root_selector = Some("article".to_string());
    cfg.exclude_selector = Some("nav,.ads".to_string());
    cfg.custom_headers = vec!["X-Test: 1".to_string()];

    let plan = plan::server_rest_plan(&cfg).expect("summarize plan");
    let req: RestSummarizeRequest =
        serde_json::from_value(plan.body.clone()).expect("valid summarize REST request");

    assert_eq!(plan.path, "/v1/summarize");
    assert_eq!(req.urls, Some(vec!["https://example.com".to_string()]));
    assert_eq!(req.render_mode, Some(RenderMode::Chrome));
    assert_eq!(req.root_selector.as_deref(), Some("article"));
    assert_eq!(req.exclude_selector.as_deref(), Some("nav,.ads"));
    assert_eq!(req.headers, vec!["X-Test: 1".to_string()]);
}

#[test]
fn extract_lifecycle_subcommands_route_to_rest_lifecycle_paths() {
    let job_id = "11111111-1111-1111-1111-111111111111";
    let cfg = cfg(CommandKind::Extract, &["cancel", job_id]);

    let plan = plan::server_rest_plan(&cfg).expect("extract cancel plan");

    assert_eq!(plan.method, "POST");
    assert_eq!(plan.path, format!("/v1/extract/{job_id}/cancel"));
}

#[tokio::test]
async fn client_server_dispatch_dead_server_fails_before_local_scrape_write() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind unused port");
    let addr = listener.local_addr().expect("local addr");
    drop(listener);

    let temp = tempfile::TempDir::new().expect("tempdir");
    let output = temp.path().join("scrape.md");
    let mut cfg = cfg(CommandKind::Scrape, &["https://example.com"]);
    cfg.server_url = Some(reqwest::Url::parse(&format!("http://{addr}")).unwrap());
    cfg.output_path = Some(output.clone());

    let err = run_server_mode_command(&cfg)
        .await
        .expect_err("dead server should fail");
    assert!(
        err.to_string().contains("start `axon serve`") && err.to_string().contains("--local"),
        "unexpected error: {err}"
    );
    assert!(
        !output.exists(),
        "server-mode dead-server path must not create local scrape output"
    );
}

#[test]
fn embed_server_mode_rejects_host_local_paths() {
    assert!(server_mode_rejects_host_local_embed_input("./README.md"));
    assert!(server_mode_rejects_host_local_embed_input("/tmp/README.md"));
    assert!(server_mode_rejects_host_local_embed_input("../README.md"));
}

#[test]
fn embed_server_mode_allows_url_and_text_inputs() {
    assert!(!server_mode_rejects_host_local_embed_input(
        "https://example.com/docs"
    ));
    assert!(!server_mode_rejects_host_local_embed_input(
        "plain text to embed"
    ));
}

#[test]
fn embed_server_mode_plan_fails_clearly_for_host_local_path() {
    let cfg = cfg(CommandKind::Embed, &["./README.md"]);

    let err = plan::server_rest_plan(&cfg).expect_err("local path should fail");
    assert!(
        err.to_string()
            .contains("server mode does not accept host-local embed paths yet"),
        "unexpected error: {err}"
    );
}

#[test]
fn extract_server_mode_plan_preserves_extract_overrides() {
    let mut cfg = cfg(CommandKind::Extract, &["https://example.com/docs"]);
    cfg.query = Some("extract facts".to_string());
    cfg.max_pages = 7;
    cfg.render_mode = RenderMode::Http;
    cfg.embed = false;

    let plan = plan::server_rest_plan(&cfg).expect("extract plan should build");

    assert_eq!(plan.path, "/v1/extract");
    assert_eq!(plan.body["urls"][0], "https://example.com/docs");
    assert_eq!(plan.body["prompt"], "extract facts");
    assert_eq!(plan.body["max_pages"], 7);
    assert_eq!(plan.body["render_mode"], "http");
    assert_eq!(plan.body["embed"], false);
}

#[test]
fn extract_server_mode_uses_direct_rest_path() {
    let mut cfg = cfg(CommandKind::Extract, &["https://example.com/docs"]);
    cfg.query = Some("extract title".to_string());
    cfg.max_pages = 1;
    cfg.embed = false;

    let plan = plan::server_rest_plan(&cfg).expect("server rest plan");

    assert_eq!(plan.method, "POST");
    assert_eq!(plan.path, "/v1/extract");
    assert_eq!(plan.body["urls"][0], "https://example.com/docs");
    assert_eq!(plan.body["max_pages"], 1);
    assert_eq!(plan.body["embed"], false);
}

#[test]
fn ingest_server_mode_uses_action_api_ingest_contract() {
    let mut cfg = cfg(
        CommandKind::Ingest,
        &["https://github.com/MCPJam/inspector"],
    );
    cfg.github_include_source = true;

    let plan = plan::server_rest_plan(&cfg).expect("ingest plan");

    assert_eq!(plan.method, "POST");
    assert_eq!(plan.path, "/v1/ingest");
    assert_eq!(
        plan.body,
        json!({
            "source_type": "github",
            "target": "MCPJam/inspector",
            "include_source": true
        })
    );
    assert!(
        plan.body.get("repo").is_none(),
        "server-mode ingest must not emit the old repo field"
    );
}

#[test]
fn sessions_server_mode_body_deserializes_as_rest_contract() {
    let mut cfg = cfg(CommandKind::Sessions, &[]);
    cfg.sessions_claude = true;
    cfg.sessions_codex = true;
    cfg.sessions_gemini = false;
    cfg.sessions_project = Some("axon_rust".to_string());

    let plan = plan::server_rest_plan(&cfg).expect("sessions plan");

    assert_eq!(plan.path, "/v1/ingest");
    let req: RestIngestRequest =
        serde_json::from_value(plan.body.clone()).expect("valid ingest REST request");
    let sessions = req.sessions.expect("sessions options");
    assert_eq!(sessions.claude, Some(true));
    assert_eq!(sessions.codex, Some(true));
    assert_eq!(sessions.gemini, Some(false));
    assert_eq!(sessions.project.as_deref(), Some("axon_rust"));
    assert_eq!(
        plan.body,
        json!({
            "source_type": "sessions",
            "sessions": {
                "claude": true,
                "codex": true,
                "gemini": false,
                "project": "axon_rust"
            }
        })
    );
    assert!(plan.body.get("sessions_claude").is_none());
}

#[test]
fn sessions_lifecycle_subcommands_route_to_ingest_job_paths() {
    let job_id = "11111111-1111-1111-1111-111111111111";
    let cfg = cfg(CommandKind::Sessions, &["status", job_id]);

    let plan = plan::server_rest_plan(&cfg).expect("sessions status plan");

    assert_eq!(plan.method, "GET");
    assert_eq!(plan.path, format!("/v1/ingest/{job_id}"));
    assert_eq!(plan.label, "ingest");
}

#[test]
fn query_server_mode_uses_direct_rest_path() {
    let mut cfg = cfg(CommandKind::Query, &[]);
    cfg.query = Some("routing contract".to_string());
    cfg.search_limit = 7;

    let plan = plan::server_rest_plan(&cfg).expect("query plan");

    assert_eq!(plan.method, "POST");
    assert_eq!(plan.path, "/v1/query");
    assert_eq!(plan.body["query"], "routing contract");
    assert_eq!(plan.body["limit"], 7);
    assert_eq!(plan.body["collection"], "axon");
}

#[test]
fn ask_server_mode_accepts_positional_text() {
    let cfg = cfg(CommandKind::Ask, &["mcp", "primitive", "prompts"]);

    let plan = plan::server_rest_plan(&cfg).expect("ask plan");

    assert_eq!(plan.method, "POST");
    assert_eq!(plan.path, "/v1/ask");
    assert_eq!(plan.body["query"], "mcp primitive prompts");
    assert_eq!(plan.body["collection"], "axon");
}

#[test]
fn sources_server_mode_preserves_limit_query() {
    let mut cfg = cfg(CommandKind::Sources, &[]);
    cfg.search_limit = 25;

    let plan = plan::server_rest_plan(&cfg).expect("sources plan");

    assert_eq!(plan.method, "GET");
    assert_eq!(plan.path, "/v1/sources?limit=25");
}

#[test]
fn server_status_text_matches_local_status_renderer() {
    let job = ServiceJob {
        id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
        status: "completed".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        started_at: None,
        finished_at: None,
        error_text: None,
        url: Some("https://example.com/docs".to_string()),
        source_type: None,
        target: Some("https://example.com/docs".to_string()),
        urls_json: None,
        result_json: Some(json!({
            "md_created": 2,
            "elapsed_ms": 1200,
            "docs_embedded": 2,
            "docs_total": 2,
            "chunks_embedded": 8
        })),
        config_json: None,
        attempt_count: 0,
        active_attempt_id: None,
        last_reclaimed_at: None,
        last_reclaimed_reason: None,
    };
    let payload = json!({
        "local_crawl_jobs": [job.clone()],
        "local_extract_jobs": [],
        "local_embed_jobs": [job],
        "local_ingest_jobs": [],
        "totals": {
            "crawl": 1,
            "extract": 0,
            "embed": 1,
            "ingest": 0
        }
    });

    let rendered = render::server_status_text(&payload).expect("status payload should render");

    assert!(rendered.contains("Crawl"));
    assert!(rendered.contains("Embed"));
    assert!(!rendered.contains("server mode"));
    assert!(rendered.contains("2 docs"));
}

#[test]
fn server_renderer_metadata_helpers_keep_display_cap() {
    let long = "x".repeat(500);

    let line = render::server_line_text(&long, 9);
    let continuation = render::server_continuation_text(&long, 4);

    assert_eq!(line.chars().count(), 111);
    assert!(line.ends_with('…'));
    assert_eq!(line.chars().count() + 9, 120);
    assert_eq!(continuation.chars().count(), 116);
    assert!(continuation.ends_with('…'));
    assert_eq!(continuation.chars().count() + 4, 120);
}

#[test]
fn server_sources_renderer_accepts_rest_tuple_rows() {
    let tuple = json!(["https://example.com/docs", 42]);
    let bare = json!("https://example.com/bare");

    assert_eq!(
        render::source_url_from_value(&tuple),
        Some("https://example.com/docs")
    );
    assert_eq!(
        render::source_url_from_value(&bare),
        Some("https://example.com/bare")
    );
    assert_eq!(render::source_url_from_value(&json!([42])), None);
}

#[test]
fn extract_status_json_promotes_completed_result_payload() {
    let result = json!({
        "job": {
            "id": "11111111-1111-1111-1111-111111111111",
            "status": "completed",
            "result_json": {
                "total_items": 1,
                "summary_path": "/tmp/extract-summary.json",
                "items_path": "/tmp/extract-items.ndjson",
                "items": [{"kind": "json-ld"}]
            }
        }
    });

    let output = render::extract_status_json_result(&result);

    assert_eq!(output["extract_result"]["total_items"], 1);
    assert_eq!(
        output["extract_result"]["items_path"],
        "/tmp/extract-items.ndjson"
    );
    assert_eq!(
        output["job"]["result_json"]["summary_path"],
        "/tmp/extract-summary.json"
    );
}
