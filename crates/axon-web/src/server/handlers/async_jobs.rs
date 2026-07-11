use axon_api::source::{AuthMode, AuthSnapshot, CallerContext, TransportKind, Visibility};
use axon_authz::VisibilityPolicy;
use axon_core::config::Config;
use axon_jobs::backend::JobKind;
use axon_services as services;
use axon_services::client_contract::{RestExtractMode, RestExtractRequest as ExtractStartRequest};
use axon_services::context::ServiceContext;
use axon_services::transport::{ExtractTransportOverrides, apply_extract_overrides};
use axum::{
    Extension, Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::post,
};
use lab_auth::AuthContext;
use serde::Serialize;
use std::sync::Arc;

use super::super::error::HttpError;
use super::super::state::AppState;
use super::jobs::job_lifecycle_router;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct AcceptedJob {
    job_id: String,
    status: &'static str,
    status_url: String,
}

type WebState = (AppState, Arc<Config>);

pub(crate) fn extract_router(service_context: Arc<ServiceContext>) -> Router<WebState> {
    Router::new()
        .route("/", post(start_extract))
        .merge(job_lifecycle_router::<WebState>(
            service_context,
            JobKind::Extract,
        ))
}

/// Validate URLs for SSRF before enqueue — rejects private-IP targets with
/// a 400 so callers learn immediately rather than after a worker run.
fn validate_ssrf_urls(urls: &[String]) -> Result<(), HttpError> {
    for url in urls {
        axon_core::http::validate_url(url)
            .map_err(|e| HttpError::bad_request(format!("{url}: {e}").as_str()))?;
    }
    Ok(())
}

#[utoipa::path(
    post,
    path = "/v1/extract",
    request_body = ExtractStartRequest,
    responses(
        (status = 202, description = "Extract job accepted", body = AcceptedJob),
        (status = 400, description = "Invalid extract request", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream extract service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "jobs"
)]
pub(crate) async fn start_extract(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    auth: Option<Extension<AuthContext>>,
    Json(req): Json<ExtractStartRequest>,
) -> Result<impl IntoResponse, HttpError> {
    if req.urls.is_empty() {
        return Err(HttpError::bad_request("urls cannot be empty"));
    }
    validate_forwarded_headers(&req.headers)?;
    if !matches!(
        req.mode.unwrap_or(RestExtractMode::Auto),
        RestExtractMode::Auto
    ) {
        return Err(HttpError::bad_request(
            "extract mode overrides are not supported by the REST job API yet",
        ));
    }
    validate_ssrf_urls(&req.urls)?;
    let cfg = apply_extract_overrides(
        &cfg,
        &ExtractTransportOverrides {
            prompt: req.prompt.clone(),
            max_pages: req.max_pages,
            render_mode: req.render_mode,
            embed: req.embed,
            collection: req.collection,
            headers: req.headers,
        },
    );
    super::rag::validate_collection_name(&cfg.collection)?;
    // Real caller identity, when present (mirrors sources.rs's
    // caller_context_from_auth): absent only in LoopbackDev mode, where the
    // loopback bind itself is the trust boundary.
    let caller_snapshot = auth.map(|Extension(auth)| {
        let auth_mode = if auth.sub == "static-bearer" {
            AuthMode::StaticToken
        } else {
            AuthMode::Oauth
        };
        let mut caller = CallerContext {
            caller_id: Some(auth.sub.clone()),
            transport: TransportKind::Rest,
            trusted_local: false,
            scopes: auth.scopes.clone(),
            visibility_ceiling: Visibility::Public,
            auth_mode,
            token_id: None,
            display_name: None,
        };
        let ceiling = VisibilityPolicy::new().ceiling_for(&caller);
        caller.visibility_ceiling = ceiling;
        AuthSnapshot::from_caller(&caller, ceiling, "runtime")
    });
    let outcome = services::extract::extract_start_with_context(
        &cfg,
        &req.urls,
        req.prompt,
        &state.service_context,
        None,
        caller_snapshot.as_ref(),
    )
    .await
    .map_err(HttpError::from_box)?;
    accepted_job("/v1/extract", outcome.result.job_id)
}

fn accepted_job(base: &str, job_id: String) -> Result<impl IntoResponse, HttpError> {
    let status_url = format!("{base}/{job_id}");
    Ok((
        StatusCode::ACCEPTED,
        [(header::LOCATION, status_url.clone())],
        Json(AcceptedJob {
            job_id,
            status: "pending",
            status_url,
        }),
    ))
}

fn validate_forwarded_headers(headers: &[String]) -> Result<(), HttpError> {
    axon_core::http::validate_custom_header_policy(headers).map_err(HttpError::bad_request)
}
