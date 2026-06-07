#[tokio::test]
async fn v1_chat_stream_rejects_empty_message() {
    let response = super::v1_chat_stream_test_response(serde_json::json!({
        "message": ""
    }))
    .await;

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn v1_chat_stream_rejects_unknown_fields() {
    let err = serde_json::from_value::<crate::services::client_contract::RestChatRequest>(
        serde_json::json!({
            "message": "hello",
            "collection": "should-not-exist"
        }),
    )
    .expect_err("chat request must reject RAG-only fields");

    assert!(err.to_string().contains("unknown field"));
}
