use axum::http::StatusCode;

#[tokio::test]
async fn ask_stream_rejects_empty_query() {
    let response = super::v1_ask_stream_test_response(serde_json::json!({
        "query": ""
    }))
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn ask_stream_rejects_explain_mode() {
    let response = super::v1_ask_stream_test_response(serde_json::json!({
        "query": "why?",
        "explain": true
    }))
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
