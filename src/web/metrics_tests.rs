use super::*;
use axum::http::{StatusCode, header};

#[tokio::test]
async fn install_is_idempotent_and_handler_serves_prometheus_exposition() {
    // `install_recorder` is the only metrics-recorder install in the crate, so
    // the first call here (or in any sibling test) installs it; subsequent calls
    // must be no-ops rather than panicking.
    install_recorder();
    install_recorder();

    let response = metrics_handler().await;
    assert_eq!(response.status(), StatusCode::OK);

    // The exact content-type is the Prometheus exposition contract — a typo
    // here silently breaks scrapers.
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .expect("metrics response must set a content-type");
    assert_eq!(content_type, "text/plain; version=0.0.4; charset=utf-8");
}
