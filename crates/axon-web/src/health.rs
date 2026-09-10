use super::server::AppState;
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use utoipa::ToSchema;

const READINESS_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[utoipa::path(
    get,
    path = "/healthz",
    responses(
        (status = 200, description = "Axon process is alive", body = String, content_type = "text/plain")
    ),
    tag = "system"
)]
pub(super) async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

#[derive(Serialize, ToSchema)]
pub(super) struct ReadinessBody {
    ok: bool,
    sqlite: &'static str,
    qdrant: &'static str,
    tei: &'static str,
}

#[utoipa::path(
    get,
    path = "/readyz",
    responses(
        (status = 200, description = "SQLite, Qdrant, and TEI dependencies are ready", body = ReadinessBody),
        (status = 503, description = "One or more dependencies are not ready", body = ReadinessBody)
    ),
    tag = "system"
)]
pub(super) async fn readyz(
    State(state): State<(AppState, Arc<axon_core::config::Config>)>,
) -> impl IntoResponse {
    let (_, cfg) = state;
    let qdrant_probe = tokio::time::timeout(
        READINESS_PROBE_TIMEOUT,
        axon_services::system::qdrant_ready(&cfg),
    );
    let tei_probe = async {
        if cfg.tei_url.trim().is_empty() {
            false
        } else {
            probe_http_endpoint(&format!("{}/health", cfg.tei_url.trim_end_matches('/'))).await
        }
    };
    let (qdrant_result, tei_ready) = tokio::join!(qdrant_probe, tei_probe);
    let qdrant_ready = qdrant_result.unwrap_or(false);
    let sqlite = axon_core::sqlite::readiness(&cfg.sqlite_path);
    let sqlite_ready = sqlite
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let (status, body) = readiness_response(sqlite_ready, qdrant_ready, tei_ready);
    (status, Json(body))
}

fn readiness_response(
    sqlite_ready: bool,
    qdrant_ready: bool,
    tei_ready: bool,
) -> (StatusCode, ReadinessBody) {
    let ok = sqlite_ready && qdrant_ready && tei_ready;
    let body = ReadinessBody {
        ok,
        sqlite: if sqlite_ready { "ready" } else { "not_ready" },
        qdrant: if qdrant_ready { "ready" } else { "not_ready" },
        tei: if tei_ready { "ready" } else { "not_ready" },
    };
    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, body)
}

async fn probe_http_endpoint(url: &str) -> bool {
    let client = match axon_core::http::internal_service_no_redirect_http_client() {
        Ok(client) => client,
        Err(_) => return false,
    };
    match tokio::time::timeout(READINESS_PROBE_TIMEOUT, client.get(url).send()).await {
        Ok(Ok(response)) => response.status().is_success(),
        Ok(Err(_)) | Err(_) => false,
    }
}

#[cfg(test)]
#[path = "health_tests.rs"]
mod tests;
