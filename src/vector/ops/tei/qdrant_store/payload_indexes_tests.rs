use super::*;
use crate::core::config::Config;
use httpmock::prelude::*;

fn make_cfg(base_url: String) -> Config {
    let mut cfg = Config::test_default();
    cfg.qdrant_url = base_url;
    cfg.collection = "test_col".to_string();
    cfg
}

fn ok_body() -> serde_json::Value {
    serde_json::json!({"result": true, "status": "ok", "time": 0.001})
}

#[tokio::test]
async fn ensure_payload_indexes_fires_one_put_per_field() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(PUT).path("/collections/test_col/index");
            then.status(200).json_body(ok_body());
        })
        .await;

    let cfg = make_cfg(server.base_url());
    ensure_payload_indexes(&cfg, None)
        .await
        .expect("should succeed");

    assert!(
        KEYWORD_INDEX_FIELDS.contains(&"chunk_content_kind"),
        "chunk_content_kind must be in the keyword index request list"
    );
    assert!(
        KEYWORD_INDEX_FIELDS.contains(&"code_chunking_method"),
        "code_chunking_method must be in the keyword index request list"
    );
    assert!(
        KEYWORD_INDEX_FIELDS.contains(&"symbol_kind"),
        "symbol_kind must be in the keyword index request list"
    );
    // keyword(N) + integer(11) + datetime(1) + bool(6) = KEYWORD_INDEX_FIELDS.len() + 18
    assert_eq!(
        mock.calls_async().await,
        KEYWORD_INDEX_FIELDS.len() + 18,
        "expected exactly one PUT per indexed field"
    );
}

#[tokio::test]
async fn ensure_payload_indexes_skips_fields_already_in_payload_schema() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(PUT).path("/collections/test_col/index");
            then.status(200).json_body(ok_body());
        })
        .await;

    // Collection info reporting every field as already indexed → zero PUTs.
    let mut schema = serde_json::Map::new();
    for field in KEYWORD_INDEX_FIELDS {
        schema.insert(
            field.to_string(),
            serde_json::json!({"data_type": "keyword"}),
        );
    }
    for field in [
        "chunk_index",
        "git_number",
        "git_comment_count",
        "git_repo_stars",
        "git_repo_forks",
        "git_repo_open_issues",
        "so_question_id",
        "payload_schema_version",
        "code_file_size_bytes",
        "code_line_start",
        "code_line_end",
        "scraped_at",
        "git_repo_is_fork",
        "git_repo_is_archived",
        "git_repo_is_private",
        "git_is_pr",
        "git_is_draft",
        "code_is_test",
    ] {
        schema.insert(
            field.to_string(),
            serde_json::json!({"data_type": "integer"}),
        );
    }
    let info = serde_json::json!({"result": {"payload_schema": schema}});

    let cfg = make_cfg(server.base_url());
    ensure_payload_indexes(&cfg, Some(&info))
        .await
        .expect("should succeed");

    assert_eq!(
        mock.calls_async().await,
        0,
        "a fully-indexed collection must issue zero index PUTs"
    );
}

#[tokio::test]
async fn ensure_payload_indexes_is_non_fatal_when_qdrant_always_errors() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(PUT).path("/collections/test_col/index");
            then.status(503)
                .json_body(serde_json::json!({"status": "error"}));
        })
        .await;

    let cfg = make_cfg(server.base_url());
    // Index assertion is an optimization: a slow/overloaded Qdrant must not
    // turn it into a failed embed. Missing indexes retry on the next embed.
    ensure_payload_indexes(&cfg, None)
        .await
        .expect("index PUT failures must not fail the embed");
}

#[tokio::test]
async fn put_index_with_retry_succeeds_on_200() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(PUT).path("/index");
            then.status(200).json_body(ok_body());
        })
        .await;

    let client = internal_service_http_client().unwrap();
    let url = format!("{}/index", server.base_url());
    let result = put_index_with_retry(
        client.clone(),
        url,
        serde_json::json!({"field_name": "url", "field_schema": "keyword"}),
    )
    .await;

    assert!(result.is_ok());
    // Exactly one request — no unnecessary retries on success.
    assert_eq!(mock.calls_async().await, 1);
}

#[tokio::test]
async fn put_index_with_retry_exhausts_all_attempts_on_persistent_error() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(PUT).path("/index");
            then.status(503)
                .json_body(serde_json::json!({"status": "error"}));
        })
        .await;

    let client = internal_service_http_client().unwrap();
    let url = format!("{}/index", server.base_url());
    let result = put_index_with_retry(
        client.clone(),
        url,
        serde_json::json!({"field_name": "url", "field_schema": "keyword"}),
    )
    .await;

    assert!(result.is_err(), "should fail after exhausting retries");
    assert_eq!(
        mock.calls_async().await,
        MAX_INDEX_ATTEMPTS as usize,
        "should attempt exactly MAX_INDEX_ATTEMPTS times"
    );
}
