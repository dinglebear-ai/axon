use super::*;

#[tokio::test]
async fn resolve_rejects_empty_source() {
    let result = resolve_source(Json(SourceRequest::new(""))).await;
    let Err(err) = result else {
        panic!("empty source must be rejected");
    };
    assert_eq!(err.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn resolve_returns_a_route_plan_for_a_web_source() {
    let response = resolve_source(Json(SourceRequest::new("https://example.com/docs")))
        .await
        .expect("a well-formed web source must resolve");
    let plan = response.0;
    assert_eq!(plan.source.canonical_uri, "https://example.com/docs");
}

#[test]
fn ledger_runtime_maps_absent_runtime_to_503_not_404() {
    let Err(err) = ledger_runtime(None) else {
        panic!("no target-local-source runtime must be an error");
    };
    assert_eq!(err.status(), StatusCode::SERVICE_UNAVAILABLE);
}
