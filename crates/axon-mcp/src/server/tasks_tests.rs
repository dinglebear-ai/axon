use super::*;
use crate::schema::{CrawlRequest, HelpRequest};
use axon_core::config::Config;

#[tokio::test]
async fn task_mode_crawl_start_rejects_multiple_urls_before_enqueue() {
    let server = AxonMcpServer::new(Config::default());
    let request = AxonRequest::Crawl(CrawlRequest {
        subaction: Some(CrawlSubaction::Start),
        urls: Some(vec![
            "https://example.com/one".to_string(),
            "https://example.com/two".to_string(),
        ]),
        ..CrawlRequest::default()
    });

    let err = enqueue_supported_start(&server, request).await.unwrap_err();
    assert!(
        err.message.contains("exactly one URL"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn unsupported_task_request_names_immediate_actions() {
    let err = unsupported_task_request(&AxonRequest::Help(HelpRequest {
        subaction: None,
        response_mode: None,
    }));
    assert!(
        err.message.contains("help"),
        "unexpected error: {}",
        err.message
    );
    assert!(
        err.message.contains("crawl.start"),
        "error should name supported task starts: {}",
        err.message
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
