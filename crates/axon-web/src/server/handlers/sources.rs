//! `POST /v1/sources` — the transport-neutral source indexing entrypoint.
//!
//! This is the REST surface for the unified source pipeline (#298). It parses
//! a JSON body into an [`axon_api::source::SourceRequest`], runs the per-source
//! authorization boundary ([`axon_authz::policy::SecurityPolicy::authorize_source`]),
//! hands the request to [`axon_services::index_source`] (which classifies +
//! acquires + dispatches to the right family bridge), and returns the resulting
//! [`axon_api::source::SourceResult`] as JSON. All legacy indexing routes
//! (`/v1/embed`, `/v1/ingest`, `/v1/scrape`, `/v1/crawl`) fold into this one
//! route per the surface-removal contract.
//!
//! ## Authorization
//!
//! The router's `require_write_scope` layer already gates this route on the
//! broad `axon:write` scope. On top of that broad gate, this handler runs the
//! *per-source* authorization boundary: it classifies the source into a
//! [`SafetyClass`] and requires the matching fine-grained scope
//! (`axon:local` for local filesystem sources, `axon:execute` for CLI/MCP tool
//! sources) via [`axon_authz::policy::ScopeSecurityPolicy`]. This is the live
//! path that actually calls `authorize_source` — broad `scope_satisfies` alone
//! cannot distinguish a local-path source from a web source.
//!
//! In `LoopbackDev` mode there is no `AuthContext` (the loopback bind is the
//! trust boundary); the per-source boundary is skipped there, matching the
//! router's decision to skip scope layers for loopback.
//!
//! The classifier (input kind -> [`SafetyClass`]) lives in
//! [`axon_services::source::classify::safety_class_for`] and the scope mapping
//! ([`SafetyClass`] -> required scope) lives in
//! `axon_authz::required_scope_for_safety_class` — both shared with the MCP
//! `source` action (`crates/axon-mcp/src/server/handlers_source.rs`), which
//! runs the equivalent boundary against `AuthSnapshot::granted_scopes` so a
//! caller cannot bypass the local-filesystem/tool-execution scope upgrade by
//! calling through MCP instead of REST.
//!
//! `index_source`'s future is not `Send` (the web-source bridge holds a
//! `Box<dyn Error>` across an `.await`), so — like `admin::run_watch` — the
//! call runs on a blocking thread via `spawn_blocking` + `Handle::block_on`,
//! whose `JoinHandle` is `Send` and thus a valid axum handler future.
//!
//! ## Async / detached execution
//!
//! `request.execution.detached == true` (and `execution.mode != Wait`) routes
//! through [`axon_services::source::enqueue::enqueue_source`] instead of
//! running acquisition inline: it creates a detached `JobKind::Source` row
//! and returns `202 Accepted` with a `SourceResult` whose `job` field carries
//! the pollable descriptor (`job_id`/`status_url`/`poll_after_ms`), matching
//! the `rest-contract.md` "Canonical Source Request" async shape. The row is
//! picked up and actually run by `SourceRunner` (registered against
//! `JobKind::Source`), which is the missing consumer side this closes (audit
//! U2-V02 / bead `axon_rust-mijoc`). `execution.mode == Wait` always forces
//! the synchronous path below, regardless of `detached`. The default
//! (`execution` omitted, `detached = false`) is unchanged: synchronous, `200
//! OK`.

use axon_api::ApiError;
use axon_api::source::{
    AuthMode, AuthSnapshot, CallerContext, ExecutionMode, SafetyClass, SecurityPolicyRequest,
    SourceRequest, SourceResult, TransportKind, Visibility,
};
use axon_authz::VisibilityPolicy;
use axon_authz::policy::{ScopeSecurityPolicy, SecurityPolicy};
use axon_authz::required_scope_for_safety_class as required_scope_for;
use axon_error::ErrorStage;
use axon_services::source::classify::{classify_source_input, safety_class_for};
use axum::{Extension, extract::State, http::StatusCode};
use lab_auth::AuthContext;
use std::sync::Arc;

use super::super::error::HttpError;
use super::super::json::Json;
use super::super::state::AppState;

type WebState = (AppState, Arc<axon_core::config::Config>);

#[utoipa::path(
    post,
    path = "/v1/sources",
    request_body = SourceRequest,
    responses(
        (status = 200, description = "Source indexing result (synchronous)", body = SourceResult),
        (status = 202, description = "Source indexing enqueued as a detached job", body = SourceResult),
        (status = 400, description = "Invalid source request", body = crate::server::error::ErrorBody),
        (status = 403, description = "Source not authorized for caller scopes", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "sources"
)]
pub(crate) async fn index_source(
    State((state, _cfg)): State<WebState>,
    auth: Option<Extension<AuthContext>>,
    Json(request): Json<SourceRequest>,
) -> Result<(StatusCode, Json<SourceResult>), HttpError> {
    if request.source.trim().is_empty() {
        // The source pipeline produces a contract `ApiError` directly; it is
        // passed through the transport verbatim as an `ErrorEnvelope`.
        return Err(HttpError::from_api_error(
            ApiError::new(
                "route.validation.missing_field",
                ErrorStage::Validation,
                "source is required",
            )
            .with_context("field", "source"),
        ));
    }

    // Per-source authorization boundary. Skipped only when there is no
    // AuthContext (LoopbackDev), matching the router's scope-layer decision.
    let auth_snapshot = if let Some(Extension(auth)) = auth {
        authorize_source_request(&request, &auth).await?;
        Some(AuthSnapshot::from_caller(
            &caller_context_from_auth(&auth),
            Visibility::Internal,
            "runtime",
        ))
    } else {
        None
    };

    let want_async = request.execution.detached && request.execution.mode != ExecutionMode::Wait;

    if want_async && let Some(job_store) = state.service_context.job_store() {
        let result = axon_services::source::enqueue::enqueue_source(
            request,
            job_store.as_ref(),
            auth_snapshot,
        )
        .await
        .map_err(|err| {
            HttpError::new(
                StatusCode::BAD_GATEWAY,
                "upstream_unavailable",
                err.to_string(),
            )
        })?;
        return Ok((StatusCode::ACCEPTED, Json(result)));
    }
    if want_async {
        // No job store configured — degrade to the synchronous path below
        // rather than failing a detached request outright.
    }

    let service_context = Arc::clone(&state.service_context);
    let handle = tokio::runtime::Handle::current();
    let result = tokio::task::spawn_blocking(move || {
        handle.block_on(async move {
            axon_services::source::index_source_with_auth(
                request,
                service_context.as_ref(),
                auth_snapshot,
            )
            .await
            .map_err(|err| err.to_string())
        })
    })
    .await
    .map_err(|err| {
        HttpError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("source indexing task failed: {err}"),
        )
    })?;
    result
        .map(|source_result| (StatusCode::OK, Json(source_result)))
        .map_err(|message| HttpError::new(StatusCode::BAD_GATEWAY, "upstream_unavailable", message))
}

/// Run the per-source authorization boundary for one [`SourceRequest`].
///
/// Classifies the source into a [`SafetyClass`], resolves the fine-grained
/// scope that class requires, and runs [`ScopeSecurityPolicy::authorize_source`]
/// against the caller's scopes. Denials surface as an enveloped
/// `auth.forbidden` error carrying the required scope.
async fn authorize_source_request(
    request: &SourceRequest,
    auth: &AuthContext,
) -> Result<(), HttpError> {
    let kind = classify_source_input(request.source.trim()).await;
    let safety_class = safety_class_for(kind);
    let required_scope = required_scope_for(safety_class);

    let policy = ScopeSecurityPolicy::new(required_scope);
    let decision = policy
        .authorize_source(SecurityPolicyRequest {
            caller: caller_context_from_auth(auth),
            safety_class,
            target: request.source.trim().to_string(),
        })
        .await
        .map_err(HttpError::from_api_error)?;

    if !decision.allowed {
        tracing::warn!(
            subject = %auth.sub,
            required_scope,
            safety_class = ?safety_class,
            reason = %decision.reason,
            "source authorization denied"
        );
        return Err(HttpError::from_api_error(
            ApiError::new(
                "auth.forbidden",
                ErrorStage::Authorizing,
                format!("source requires scope: {required_scope}"),
            )
            .with_context("required_scope", required_scope)
            .with_context("safety_class", safety_class_str(safety_class)),
        ));
    }
    Ok(())
}

fn caller_context_from_auth(auth: &AuthContext) -> CallerContext {
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
    caller.visibility_ceiling = VisibilityPolicy::new().ceiling_for(&caller);
    caller
}

// `safety_class_for` (input kind -> `SafetyClass`) and `required_scope_for`
// (`SafetyClass` -> required scope, aliased above from
// `axon_authz::required_scope_for_safety_class`) both now live in shared
// crates so REST and MCP (`crates/axon-mcp/src/server/handlers_source.rs`)
// authorize a source with the exact same classifier and scope mapping — see
// `axon_services::source::classify::safety_class_for`'s doc comment for why
// that matters.

fn safety_class_str(safety_class: SafetyClass) -> &'static str {
    match safety_class {
        SafetyClass::PublicNetwork => "public_network",
        SafetyClass::AuthenticatedNetwork => "authenticated_network",
        SafetyClass::LocalFilesystem => "local_filesystem",
        SafetyClass::ToolExecution => "tool_execution",
    }
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
