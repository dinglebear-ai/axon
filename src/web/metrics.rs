use crate::core::logging::log_warn;
use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::sync::OnceLock;

static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Install the Prometheus recorder once at server startup.
///
/// Idempotent: a second call is a no-op. Must be called before the `/metrics`
/// route is first hit (wired in `run_unified_server`). On failure — e.g. a
/// global `metrics` recorder is already installed — this logs a warning and
/// leaves `/metrics` returning 503 rather than panicking the server.
pub(crate) fn install_recorder() {
    if PROMETHEUS_HANDLE.get().is_some() {
        return;
    }
    match PrometheusBuilder::new().install_recorder() {
        Ok(handle) => {
            // `set` only races with a concurrent install; the startup call site
            // is single-threaded, so a spurious Err here cannot happen in
            // practice and would be harmless if it did.
            let _ = PROMETHEUS_HANDLE.set(handle);
        }
        Err(e) => log_warn(&format!(
            "metrics: failed to install Prometheus recorder: {e}; /metrics will return 503"
        )),
    }
}

pub(super) async fn metrics_handler() -> Response {
    let Some(handle) = PROMETHEUS_HANDLE.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "metrics not initialized").into_response();
    };
    let body = handle.render();
    let mut response = (StatusCode::OK, body).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    response
}

#[cfg(test)]
#[path = "metrics_tests.rs"]
mod tests;
