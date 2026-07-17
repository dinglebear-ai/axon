use super::*;
use crate::schema::{HelpRequest, parse_axon_request};

#[test]
fn unsupported_task_request_names_immediate_actions() {
    let err = unsupported_task_request(&AxonRequest::Help(HelpRequest {
        response_mode: None,
    }));
    assert!(
        err.message.contains("help"),
        "unexpected error: {}",
        err.message
    );
    assert!(
        err.message.contains("extract.start"),
        "error should name the supported task start: {}",
        err.message
    );
}

#[test]
fn task_mode_removed_crawl_fails_before_task_dispatch() {
    let raw = serde_json::json!({
        "action": "crawl",
        "subaction": "start",
        "urls": ["https://example.com/one"]
    })
    .as_object()
    .expect("object")
    .clone();

    let err = parse_axon_request(raw).expect_err("removed crawl must not parse");
    assert!(
        err.contains("action `crawl` was removed from MCP") && err.contains("action=source"),
        "removed crawl should fail closed with replacement guidance: {err}"
    );
}

#[test]
fn task_list_cursor_rejects_offsets_past_cap() {
    assert_eq!(parse_cursor_offset(Some("200".to_string())).unwrap(), 200);
    let err = parse_cursor_offset(Some("220".to_string())).unwrap_err();
    assert!(
        err.message.contains("<= 200"),
        "unexpected error: {}",
        err.message
    );
}
