use axon_api::reset::{RESET_STORE_ARTIFACTS, ResetPlan, ResetResult};
use axon_api::source::ids::{SourceGenerationId, SourceId};
use axon_api::source::prune::{PruneRequest as ApiPruneRequest, PruneResult, PruneSelector};
use axon_core::config::Config;
use axon_services as services;
use axon_services::prune::PruneAuthz;
use axon_services::reset::ResetAuthz;
use axum::{Extension, Json, extract::State, http::StatusCode};
use lab_auth::AuthContext;
use serde::Deserialize;
use std::sync::Arc;

use super::super::error::HttpError;

type WebState = (super::super::state::AppState, Arc<Config>);

const PRUNE_COLLECTION_PREFIX: &str = "collection:";

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
    pub prune_plan_id: String,
    #[serde(default)]
    pub confirm: bool,
    #[serde(default)]
    pub reason: String,
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
    validate_reason(&req.reason)?;
    let plan_id = req.prune_plan_id.trim();
    if plan_id.is_empty() {
        return Err(HttpError::bad_request("prune_plan_id is required"));
    }
    let authz = prune_authz_from_auth(auth.as_ref());
    let (_, result, _) =
        services::prune::prune_execute_saved(&state.service_context, plan_id, req.confirm, &authz)
            .await
            .map_err(destructive_error)?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResetPlanRequest {
    #[serde(default)]
    pub stores: Vec<String>,
    #[serde(default = "default_true")]
    pub dry_run: bool,
    pub collection: Option<String>,
    pub include_artifacts: Option<bool>,
    #[serde(default)]
    pub include_config: bool,
    #[serde(default)]
    pub reason: String,
}

impl Default for ResetPlanRequest {
    fn default() -> Self {
        Self {
            stores: Vec::new(),
            dry_run: true,
            collection: None,
            include_artifacts: None,
            include_config: false,
            reason: String::new(),
        }
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResetExecRequest {
    pub reset_plan_id: String,
    #[serde(default)]
    pub confirm: bool,
    #[serde(default)]
    pub reason: String,
}

#[utoipa::path(
    post,
    path = "/v1/reset/plan",
    operation_id = "plan_reset",
    request_body = ResetPlanRequest,
    responses(
        (status = 200, description = "Reviewable reset plan", body = ResetPlan),
        (status = 400, description = "Invalid reset plan request", body = crate::server::error::ErrorBody),
        (status = 403, description = "Caller lacks axon:admin", body = crate::server::error::ErrorBody)
    ),
    tag = "admin"
)]
pub(crate) async fn reset_plan(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<ResetPlanRequest>,
) -> Result<Json<ResetPlan>, HttpError> {
    validate_reason(&req.reason)?;
    let plan_cfg = reset_plan_config(cfg.as_ref(), &req)?;
    let result = services::reset::reset_with_authz(&plan_cfg, &ResetAuthz::anonymous())
        .await
        .map_err(destructive_error)?;
    Ok(Json(result.reset_plan))
}

#[utoipa::path(
    post,
    path = "/v1/reset/exec",
    operation_id = "execute_reset",
    request_body = ResetExecRequest,
    responses(
        (status = 200, description = "Reset execution receipt", body = ResetResult),
        (status = 400, description = "Missing confirmation or invalid plan", body = crate::server::error::ErrorBody),
        (status = 403, description = "Caller lacks axon:admin", body = crate::server::error::ErrorBody),
        (status = 409, description = "Reviewed reset plan no longer matches inventory or configuration", body = crate::server::error::ErrorBody)
    ),
    tag = "admin"
)]
pub(crate) async fn reset_exec(
    State((_state, cfg)): State<WebState>,
    auth: Option<Extension<AuthContext>>,
    Json(req): Json<ResetExecRequest>,
) -> Result<Json<ResetResult>, HttpError> {
    if !req.confirm {
        return Err(HttpError::bad_request(
            "reset exec requires confirm=true to run destructively",
        ));
    }
    validate_reason(&req.reason)?;
    let plan_id = req.reset_plan_id.trim();
    if plan_id.is_empty() {
        return Err(HttpError::bad_request("reset_plan_id is required"));
    }
    let mut exec_cfg = (*cfg).clone();
    exec_cfg.reset_dry_run = false;
    exec_cfg.yes = true;
    exec_cfg.reset_plan_id = Some(plan_id.to_string());
    let authz = reset_authz_from_auth(auth.as_ref());
    services::reset::reset_with_authz(&exec_cfg, &authz)
        .await
        .map(Json)
        .map_err(destructive_error)
}

fn reset_plan_config(cfg: &Config, req: &ResetPlanRequest) -> Result<Config, HttpError> {
    if !req.dry_run {
        return Err(HttpError::bad_request(
            "reset planning requires dry_run=true; use /v1/reset/exec to execute",
        ));
    }
    if req.include_config {
        return Err(HttpError::bad_request(
            "configuration is not a resettable store",
        ));
    }
    if let Some(collection) = req.collection.as_deref() {
        axon_core::config::validate_collection_name(collection)
            .map_err(|error| HttpError::bad_request(format!("collection: {error}")))?;
        if collection != cfg.collection {
            return Err(HttpError::bad_request(
                "collection must match the configured collection for plan-id execution",
            ));
        }
    }
    let mut plan_cfg = cfg.clone();
    plan_cfg.reset_dry_run = true;
    plan_cfg.yes = false;
    plan_cfg.reset_plan_id = None;
    plan_cfg.reset_stores = req.stores.clone();
    if req.include_artifacts == Some(false) {
        if plan_cfg.reset_stores.is_empty() {
            plan_cfg.reset_stores = axon_api::reset::RESET_ALL_STORES
                .iter()
                .filter(|store| **store != RESET_STORE_ARTIFACTS)
                .map(|store| (*store).to_string())
                .collect();
        } else {
            plan_cfg
                .reset_stores
                .retain(|store| store != RESET_STORE_ARTIFACTS);
            if plan_cfg.reset_stores.is_empty() {
                return Err(HttpError::bad_request(
                    "no reset stores remain after include_artifacts=false",
                ));
            }
        }
    } else if req.include_artifacts == Some(true)
        && !plan_cfg.reset_stores.is_empty()
        && !plan_cfg
            .reset_stores
            .iter()
            .any(|store| store == RESET_STORE_ARTIFACTS)
    {
        plan_cfg
            .reset_stores
            .push(RESET_STORE_ARTIFACTS.to_string());
    }
    Ok(plan_cfg)
}

const fn default_true() -> bool {
    true
}

fn validate_reason(reason: &str) -> Result<(), HttpError> {
    if reason.len() > 1024 {
        return Err(HttpError::bad_request("reason must be 1024 bytes or fewer"));
    }
    Ok(())
}

fn prune_authz_from_auth(auth: Option<&Extension<AuthContext>>) -> PruneAuthz {
    match auth {
        Some(Extension(auth_ctx)) => PruneAuthz {
            is_admin: axon_authz::scope_satisfies(&auth_ctx.scopes, axon_authz::AXON_ADMIN_SCOPE),
        },
        None => PruneAuthz::admin(),
    }
}

fn reset_authz_from_auth(auth: Option<&Extension<AuthContext>>) -> ResetAuthz {
    match auth {
        Some(Extension(auth_ctx)) => ResetAuthz {
            is_admin: axon_authz::scope_satisfies(&auth_ctx.scopes, axon_authz::AXON_ADMIN_SCOPE),
        },
        None => ResetAuthz::admin(),
    }
}

fn destructive_error(error: impl std::fmt::Display) -> HttpError {
    let message = error.to_string();
    let (status, kind) = if message.contains("admin_required") {
        (StatusCode::FORBIDDEN, "forbidden")
    } else if message.contains("inventory_changed") || message.contains("config_changed") {
        (StatusCode::CONFLICT, "bad_request")
    } else {
        (StatusCode::BAD_REQUEST, "bad_request")
    };
    HttpError::new(status, kind, message)
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

#[cfg(test)]
#[path = "admin_tests.rs"]
mod tests;
