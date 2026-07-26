use axum::response::IntoResponse;

#[test]
fn v1_chat_rejects_empty_message() {
    let error = super::validate_chat_message("   ").expect_err("empty chat must fail");
    assert_eq!(
        error.into_response().status(),
        axum::http::StatusCode::BAD_REQUEST
    );
}
