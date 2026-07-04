use super::*;
use axum::body::to_bytes;

async fn body_json(response: Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    serde_json::from_slice(&bytes).expect("body is JSON")
}

#[tokio::test]
async fn not_found_fallback_is_enveloped() {
    let response = not_found_fallback().await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let value = body_json(response).await;
    assert_eq!(value["ok"], serde_json::json!(false));
    assert_eq!(value["error"]["code"], "route.not_found");
    assert_eq!(value["error"]["stage"], "routing");
}

#[tokio::test]
async fn method_not_allowed_fallback_is_enveloped() {
    let response = method_not_allowed_fallback().await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    let value = body_json(response).await;
    assert_eq!(value["ok"], serde_json::json!(false));
    assert_eq!(value["error"]["code"], "route.method_not_allowed");
    assert_eq!(value["error"]["stage"], "routing");
}

/// A malformed body (missing a required field on a `deny_unknown_fields`
/// struct) must serialize as the contract envelope with the invalid-body code,
/// not axum's raw plaintext rejection. Exercises the extractor directly so no
/// HTTP server or extra tower dev-dependency is required.
#[tokio::test]
async fn missing_field_rejection_is_enveloped() {
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Body {
        #[allow(dead_code)]
        source: String,
    }

    let request = Request::builder()
        .method("POST")
        .uri("/x")
        .header("content-type", "application/json")
        .body(axum::body::Body::from("{}"))
        .expect("build request");

    let response = match Json::<Body>::from_request(request, &()).await {
        Ok(_) => panic!("missing required field must be rejected"),
        Err(response) => response,
    };
    assert!(response.status().is_client_error());
    let value = body_json(response).await;
    assert_eq!(value["ok"], serde_json::json!(false));
    assert_eq!(value["error"]["code"], "route.validation.invalid_body");
    assert_eq!(value["error"]["stage"], "validation");
}
