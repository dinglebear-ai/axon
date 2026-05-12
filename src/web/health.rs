use super::server::AppState;
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

pub(super) async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

#[derive(Serialize)]
struct ReadinessBody {
    ok: bool,
    qdrant: &'static str,
    tei: &'static str,
}

pub(super) async fn readyz(
    State((_, cfg)): State<(AppState, Arc<crate::core::config::Config>)>,
) -> impl IntoResponse {
    let qdrant_ready =
        probe_http_endpoint(&format!("{}/readyz", cfg.qdrant_url.trim_end_matches('/'))).await;
    let tei_ready = if cfg.tei_url.trim().is_empty() {
        false
    } else {
        probe_http_endpoint(&format!("{}/health", cfg.tei_url.trim_end_matches('/'))).await
    };
    let ok = qdrant_ready && tei_ready;
    let body = ReadinessBody {
        ok,
        qdrant: if qdrant_ready { "ready" } else { "not_ready" },
        tei: if tei_ready { "ready" } else { "not_ready" },
    };
    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(body))
}

async fn probe_http_endpoint(url: &str) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };
    client
        .get(url)
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}
