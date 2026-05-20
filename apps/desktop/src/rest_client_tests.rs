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
