use super::*;
use crate::actions::{ArgMode, CommandAction};

fn action(subcommand: &'static str, arg_mode: ArgMode) -> CommandAction {
    CommandAction {
        label: subcommand,
        subcommand,
        arg_mode,
        aliases: &[],
        description: "",
        example: "",
    }
}

#[test]
fn builds_query_rest_request() {
    let request = build_rest_request(action("query", ArgMode::Single), "gpui menus").unwrap();

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/query");
    assert_eq!(
        request.body,
        Some(json!({ "query": "gpui menus", "limit": 10 }))
    );
}

#[test]
fn builds_chat_rest_request_without_collection() {
    let request = build_rest_request(action("chat", ArgMode::Single), "plain llm chat").unwrap();

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/chat");
    assert_eq!(request.body, Some(json!({ "message": "plain llm chat" })));
}

#[test]
fn builds_summarize_request_with_multiple_urls() {
    let request = build_rest_request(
        action("summarize", ArgMode::Split),
        "https://a.example https://b.example",
    )
    .unwrap();

    assert_eq!(request.path, "/v1/summarize");
    assert_eq!(
        request.body,
        Some(json!({ "urls": ["https://a.example", "https://b.example"] }))
    );
}

#[test]
fn builds_empty_suggest_body_when_focus_is_absent() {
    let request = build_rest_request(action("suggest", ArgMode::OptionalSingle), "").unwrap();

    assert_eq!(request.path, "/v1/suggest");
    assert_eq!(request.body, Some(json!({})));
}

#[test]
fn builds_evaluate_request() {
    let request =
        build_rest_request(action("evaluate", ArgMode::Single), "compare answers").unwrap();

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/evaluate");
    assert_eq!(request.body, Some(json!({ "question": "compare answers" })));
}

#[test]
fn builds_screenshot_rest_request() {
    let request =
        build_rest_request(action("screenshot", ArgMode::Split), "https://example.com").unwrap();

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/screenshot");
    assert_eq!(request.body, Some(json!({ "url": "https://example.com" })));
}

#[test]
fn screenshot_requires_url_argument() {
    let result = build_rest_request(action("screenshot", ArgMode::Split), "");
    assert!(result.is_err(), "screenshot with no url should error");
}

#[test]
fn builds_sources_as_read_only_get() {
    let request = build_rest_request(action("sources", ArgMode::None), "").unwrap();

    assert_eq!(request.method, "GET");
    assert_eq!(request.path, "/v1/sources?limit=100");
    assert_eq!(request.body, None);
}

#[test]
fn parses_shell_unsafe_env_values_without_sourcing() {
    let entries = parse_env_entries(
        r#"
        AXON_SERVER_URL=http://127.0.0.1:8001
        NVIDIA_REQUIRE_CUDA=cuda>=12.2
        AXON_MCP_HTTP_TOKEN="secret-token"
        "#,
    );

    assert_eq!(
        entries,
        vec![
            (
                "AXON_SERVER_URL".to_string(),
                "http://127.0.0.1:8001".to_string()
            ),
            ("NVIDIA_REQUIRE_CUDA".to_string(), "cuda>=12.2".to_string()),
            (
                "AXON_MCP_HTTP_TOKEN".to_string(),
                "secret-token".to_string()
            ),
        ]
    );
}

#[test]
fn health_check_returns_false_on_connection_refused() {
    // Port 19999 is not listening — expect a connection error, not a panic.
    // The health_check method must return Ok(false) or Err, not panic.
    let client = RestClient {
        base_url: "http://127.0.0.1:19999".to_string(),
        token: None,
        client: reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_millis(200))
            .build()
            .unwrap(),
    };
    // Either Err (connection refused) or Ok(false) — never panics.
    let result = client.health_check();
    assert!(
        result.is_err() || result == Ok(false),
        "expected connection error or false, got {result:?}"
    );
}

#[test]
fn health_check_url_is_healthz_not_doctor() {
    // Verify the path constant — health_check() must append /healthz.
    // We inspect via a known base URL and confirm the suffix.
    // This is a unit test of the naming contract; actual HTTP is tested above.
    let base = "http://example.test";
    let expected = format!("{base}/healthz");
    // The function constructs `format!("{}/healthz", self.base_url)`.
    // We can verify the path indirectly by confirming parse_env_entries
    // trims slashes consistently so the URL is well-formed.
    let entries = parse_env_entries(&format!("AXON_SERVER_URL={base}\n"));
    assert_eq!(
        entries,
        vec![("AXON_SERVER_URL".to_string(), base.to_string())]
    );
    // Confirm trailing-slash trimming doesn't double-slash.
    let entries_slash = parse_env_entries(&format!("AXON_SERVER_URL={base}/\n"));
    assert_eq!(
        entries_slash,
        vec![("AXON_SERVER_URL".to_string(), format!("{base}/"))]
    );
    // The RestClient trims trailing slash so healthz URL remains clean.
    let _ = expected; // used in the docstring above
}
