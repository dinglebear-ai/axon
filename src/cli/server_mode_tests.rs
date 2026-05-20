use super::*;
use crate::core::config::CommandKind;
use crate::core::config::RenderMode;
use crate::services::types::ServiceJob;
use chrono::Utc;
use serde_json::json;
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
fn scrape_server_mode_uses_rest_contract_body() {
    let mut cfg = cfg(CommandKind::Scrape, &["https://example.com"]);
    cfg.embed = true;
    cfg.render_mode = RenderMode::Chrome;

    let plan = plan::server_rest_plan(&cfg).expect("scrape plan");

    assert_eq!(plan.path, "/v1/scrape");
    assert_eq!(plan.body, json!({ "url": "https://example.com" }));
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
fn query_server_mode_uses_direct_rest_path() {
    let mut cfg = cfg(CommandKind::Query, &[]);
    cfg.query = Some("routing contract".to_string());
    cfg.search_limit = 7;

    let plan = plan::server_rest_plan(&cfg).expect("query plan");

    assert_eq!(plan.method, "POST");
    assert_eq!(plan.path, "/v1/query");
    assert_eq!(plan.body["query"], "routing contract");
    assert_eq!(plan.body["limit"], 7);
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
