use axon_api::mcp_schema::PurgeRequest;
use axon_api::source::ids::{SourceGenerationId, SourceId};
use axon_api::source::prune::{PruneRequest as ApiPruneRequest, PruneResult, PruneSelector};
use axon_core::config::Config;
use axon_services as services;
use axon_services::prune::PruneAuthz;
use axum::{
    Extension, Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
};
use lab_auth::AuthContext;
use serde::Deserialize;
use std::sync::Arc;

use super::super::error::HttpError;

type WebState = (super::super::state::AppState, Arc<Config>);

const PRUNE_COLLECTION_PREFIX: &str = "collection:";

#[derive(Debug, Deserialize, Default, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DedupeRequest {
    collection: Option<String>,
}

#[utoipa::path(
    post,
    path = "/v1/prune/dedupe",
    request_body(content = Option<DedupeRequest>, content_type = "application/json"),
    responses(
        (status = 200, description = "Dedupe result", body = serde_json::Value),
        (status = 400, description = "Invalid dedupe request", body = crate::server::error::ErrorBody),
        (status = 403, description = "Caller lacks axon:admin", body = crate::server::error::ErrorBody),
        (status = 415, description = "Unsupported request body content type", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "admin"
)]
pub(crate) async fn dedupe(
    State((_state, cfg)): State<WebState>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<services::types::DedupeResult>, HttpError> {
    let mut req_cfg = (*cfg).clone();
    if let Some(req) = parse_optional_json_body::<DedupeRequest>(&headers, &body)?
        && let Some(collection) = req.collection
    {
        axon_core::config::validate_collection_name(&collection)
            .map_err(|e| HttpError::bad_request(format!("collection: {e}").as_str()))?;
        req_cfg.collection = collection;
    }
    services::system::dedupe(&req_cfg, None)
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

#[utoipa::path(
    post,
    path = "/v1/prune/purge",
    request_body = PurgeRequest,
    responses(
        (status = 200, description = "Purge result (counts of points/URLs matched or deleted)", body = axon_api::PurgeResult),
        (status = 400, description = "Invalid purge request", body = crate::server::error::ErrorBody),
        (status = 403, description = "Caller lacks axon:admin", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "admin"
)]
pub(crate) async fn purge(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<PurgeRequest>,
) -> Result<Json<services::types::PurgeResult>, HttpError> {
    let target = req
        .target
        .as_deref()
        .map(str::trim)
        .filter(|target| !target.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| HttpError::bad_request("target is required"))?;
    let mut req_cfg = (*cfg).clone();
    if let Some(collection) = req.collection {
        axon_core::config::validate_collection_name(&collection)
            .map_err(|e| HttpError::bad_request(format!("collection: {e}").as_str()))?;
        req_cfg.collection = collection;
    }
    services::system::purge(&req_cfg, &target, req.prefix, req.dry_run.unwrap_or(true))
        .await
        .map(Json)
        .map_err(HttpError::from_box)
}

/// Body for `POST /v1/prune/plan` — a dry-run plan, always safe to call.
///
/// `target` is either a bare source id or `collection:<name>` for a
/// whole-collection prune, mirroring the CLI's `axon prune plan <target>`
/// selector grammar (`crates/axon-cli/src/commands/prune.rs::build_selector`).
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrunePlanRequest {
    pub target: String,
    pub generation: Option<String>,
}

/// Body for `POST /v1/prune/exec` — destructive; requires `axon:admin` (see
/// [`super::super::routing::admin_routes`]) and `confirm: true`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PruneExecRequest {
    pub target: String,
    pub generation: Option<String>,
    #[serde(default)]
    pub confirm: bool,
}

#[utoipa::path(
    post,
    path = "/v1/prune/plan",
    request_body = PrunePlanRequest,
    responses(
        (status = 200, description = "Dry-run prune plan (never mutates state)", body = axon_api::source::prune::PrunePlan),
        (status = 400, description = "Invalid prune request", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "admin"
)]
pub(crate) async fn prune_plan(
    State((state, _cfg)): State<WebState>,
    Json(req): Json<PrunePlanRequest>,
) -> Result<Json<axon_api::source::prune::PrunePlan>, HttpError> {
    let selector = prune_selector_from_body(&req.target, req.generation.as_deref())?;
    let request = ApiPruneRequest::dry_run(selector, "rest prune plan");
    // Dry-run planning never mutates state and never checks authz — mirrors
    // `axon_services::prune::prune_plan`'s own contract. Uses the real,
    // ledger-backed estimate (`prune_plan_estimated`) rather than the
    // always-zero `NullScopeSource` fallback, since a `ServiceContext` (and
    // therefore a ledger handle) is available here.
    let plan = services::prune::prune_plan_estimated(&state.service_context, &request).await;
    Ok(Json(plan))
}

#[utoipa::path(
    post,
    path = "/v1/prune/exec",
    request_body = PruneExecRequest,
    responses(
        (status = 200, description = "Prune execution result (receipt of what was actually deleted)", body = PruneResult),
        (status = 400, description = "Invalid prune request, or missing confirm=true", body = crate::server::error::ErrorBody),
        (status = 403, description = "Caller lacks axon:admin", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "admin"
)]
pub(crate) async fn prune_exec(
    State((state, _cfg)): State<WebState>,
    auth: Option<Extension<AuthContext>>,
    Json(req): Json<PruneExecRequest>,
) -> Result<Json<PruneResult>, HttpError> {
    if !req.confirm {
        return Err(HttpError::bad_request(
            "prune exec requires confirm=true to run destructively",
        ));
    }
    let selector = prune_selector_from_body(&req.target, req.generation.as_deref())?;
    let api_request = ApiPruneRequest::execute(selector, "rest prune exec");

    // `axon:admin` derived from the caller's real resolved scopes — never
    // hardcoded. The router's `admin_routes` layer (require_admin_scope) has
    // already rejected non-admin Mounted callers before this handler runs;
    // re-deriving here (rather than assuming `PruneAuthz::admin()`) keeps
    // this handler honest on its own and correct if the router layer is ever
    // relaxed. `LoopbackDev` has no `AuthContext` at all — the loopback bind
    // itself is the trust boundary there, matching every other admin route.
    let authz = match auth {
        Some(Extension(ref auth_ctx)) => PruneAuthz {
            is_admin: axon_authz::scope_satisfies(&auth_ctx.scopes, axon_authz::AXON_ADMIN_SCOPE),
        },
        None => PruneAuthz::admin(),
    };

    let (_plan, result) = services::prune::prune(&state.service_context, &api_request, &authz)
        .await
        .map_err(|err| HttpError::new(StatusCode::FORBIDDEN, "forbidden", err.to_string()))?;
    let result = result.ok_or_else(|| {
        HttpError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "prune exec did not produce a result",
        )
    })?;
    Ok(Json(result))
}

/// Build a [`PruneSelector`] from a REST body's `target`/`generation` fields.
/// Mirrors `crates/axon-cli/src/commands/prune.rs::build_selector`'s
/// `collection:<name>` / bare-source-id grammar.
fn prune_selector_from_body(
    target: &str,
    generation: Option<&str>,
) -> Result<PruneSelector, HttpError> {
    let target = target.trim();
    if target.is_empty() {
        return Err(HttpError::bad_request(
            "target is required (source id, or collection:<name>)",
        ));
    }

    if let Some(collection) = target.strip_prefix(PRUNE_COLLECTION_PREFIX) {
        let collection = collection.trim();
        if collection.is_empty() {
            return Err(HttpError::bad_request(
                "collection: target requires a non-empty collection name",
            ));
        }
        if generation.is_some() {
            return Err(HttpError::bad_request(
                "generation is not valid with a collection: target",
            ));
        }
        return Ok(PruneSelector::Collection {
            collection: collection.to_string(),
        });
    }

    let source_id = SourceId::new(target);
    Ok(match generation.map(str::trim).filter(|g| !g.is_empty()) {
        Some(generation) => PruneSelector::Generation {
            source_id,
            generation: SourceGenerationId::new(generation),
        },
        None => PruneSelector::Source { source_id },
    })
}

fn parse_optional_json_body<T>(headers: &HeaderMap, body: &str) -> Result<Option<T>, HttpError>
where
    T: serde::de::DeserializeOwned,
{
    if body.is_empty() {
        return Ok(None);
    }
    if !has_json_content_type(headers) {
        return Err(HttpError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "non-empty request body must use application/json",
        ));
    }
    serde_json::from_str(body)
        .map(Some)
        .map_err(|e| HttpError::bad_request(format!("invalid JSON request body: {e}")))
}

fn has_json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            let media_type = value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            media_type == "application/json" || media_type.ends_with("+json")
        })
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "admin_tests.rs"]
mod tests;
