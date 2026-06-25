use super::server::AppState;
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use utoipa::ToSchema;

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
    let qdrant_ready =
        probe_http_endpoint(&format!("{}/readyz", cfg.qdrant_url.trim_end_matches('/'))).await;
    let tei_ready = if cfg.tei_url.trim().is_empty() {
        false
    } else {
        probe_http_endpoint(&format!("{}/health", cfg.tei_url.trim_end_matches('/'))).await
    };
    let sqlite = axon_jobs::store::sqlite_readiness(&cfg.sqlite_path);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readyz_response_includes_sqlite_dependency() {
        let (status, body) = readiness_response(false, true, true);

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(!body.ok);
        assert_eq!(body.sqlite, "not_ready");
        assert_eq!(body.qdrant, "ready");
        assert_eq!(body.tei, "ready");
    }

    #[test]
    fn readyz_response_is_ok_only_when_all_dependencies_are_ready() {
        let (status, body) = readiness_response(true, true, true);

        assert_eq!(status, StatusCode::OK);
        assert!(body.ok);
        assert_eq!(body.sqlite, "ready");
    }
}
