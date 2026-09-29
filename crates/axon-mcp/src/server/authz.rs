use crate::auth::AuthPolicy;
use axon_authz::{has_explicit_scope, scope_satisfies};
use lab_auth::AuthContext;
use rmcp::{ErrorData, RoleServer, service::RequestContext};

#[path = "action_specs.rs"]
mod action_specs;
pub(crate) use action_specs::{MCP_ACTION_SPECS, McpActionSpec, McpSafetyHints};

pub(crate) fn mcp_action_names() -> Vec<&'static str> {
    MCP_ACTION_SPECS.iter().map(|spec| spec.name).collect()
}

/// Extract and enforce the authentication context from the rmcp request.
///
/// `LoopbackDev` trusts process isolation. Mounted HTTP mode requires the auth
/// middleware to have inserted an `AuthContext` into request extensions.
pub(crate) fn require_auth_context<'a>(
    policy: &AuthPolicy,
    ctx: &'a RequestContext<RoleServer>,
) -> Result<Option<&'a AuthContext>, ErrorData> {
    match policy {
        AuthPolicy::LoopbackDev => Ok(None),
        AuthPolicy::Mounted { .. } => {
            let parts = ctx
                .extensions
                .get::<axum::http::request::Parts>()
                .ok_or_else(|| {
                    tracing::error!(
                        "rmcp HTTP Parts extension absent — middleware ordering may be broken"
                    );
                    ErrorData::invalid_request("forbidden: missing http context", None)
                })?;
            let auth = parts.extensions.get::<AuthContext>().ok_or_else(|| {
                tracing::warn!(
                    "AuthContext absent from request extensions — \
                     AuthLayer may not be mounted or rejected the request without inserting context"
                );
                ErrorData::invalid_request("forbidden: missing auth context", None)
            })?;
            Ok(Some(auth))
        }
    }
}

/// Enforce that `auth` carries `required_scope`.
///
/// OAuth email allowlisting is the access boundary. Any valid Axon OAuth scope
/// grants full Axon server access; scope names remain for client compatibility.
pub(crate) fn check_scope(
    auth: &AuthContext,
    required_scope: &str,
    action: &str,
) -> Result<(), ErrorData> {
    let satisfied = scope_satisfies(&auth.scopes, required_scope);
    if satisfied {
        return Ok(());
    }
    tracing::warn!(
        subject = %auth.sub,
        action = %action,
        required_scope = %required_scope,
        "MCP tool invocation denied: insufficient scope"
    );
    Err(ErrorData::invalid_request(
        format!("forbidden: requires scope: {required_scope}"),
        None,
    ))
}

/// Strict counterpart to [`check_scope`] for conditional scope *elevation*
/// checks only (see [`mutates_if_upgrade`]).
///
/// `check_scope` calls `axon_authz::scope_satisfies`, which deliberately
/// treats `axon:read` and `axon:write` as interchangeable for ordinary broad
/// read/write route gating (OAuth dual-scope compatibility — see
/// `docs/pipeline-unification/runtime/security-contract.md`'s "Contract"
/// paragraph and `docs/pipeline-unification/runtime/auth-contract.md`'s
/// "Scope Rules"). That widening is correct for ordinary routes, but it makes
/// `mutates_if_upgrade`'s elevation a silent no-op if reused here: a caller
/// holding only `axon:read` already "satisfies" a required `axon:write`
/// before the elevation even matters, defeating the entire point of
/// upgrading `search`/`research` to `axon:write` (CWE-863). This function
/// uses `axon_authz::has_explicit_scope` instead, which requires the caller
/// to hold the exact elevated scope with no broad-scope widening. Use this
/// only where [`required_scope_with_mutates_if`] actually applied an
/// elevation — use `check_scope` for every ordinary action/subaction scope
/// check.
pub(crate) fn check_scope_explicit(
    auth: &AuthContext,
    required_scope: &str,
    action: &str,
) -> Result<(), ErrorData> {
    if has_explicit_scope(&auth.scopes, required_scope) {
        return Ok(());
    }
    tracing::warn!(
        subject = %auth.sub,
        action = %action,
        required_scope = %required_scope,
        "MCP tool invocation denied: insufficient scope (explicit elevation check)"
    );
    Err(ErrorData::invalid_request(
        format!("forbidden: requires scope: {required_scope}"),
        None,
    ))
}

/// Map an axon tool action and subaction to the minimum required scope.
pub fn required_scope_for(action: &str, subaction: &str) -> Option<&'static str> {
    if action == "reset" {
        return match subaction {
            "" | "plan" | "get" | "exec" => Some("axon:admin"),
            _ => Some("__deny__"),
        };
    }
    if action == "collections" {
        return match subaction {
            "" | "list" | "get" => Some("axon:read"),
            _ => Some("__deny__"),
        };
    }
    if action == "uploads" {
        return match subaction {
            "" | "list" | "get" => Some("axon:read"),
            "create" | "put_content" | "complete" | "abort" => Some("axon:write"),
            _ => Some("__deny__"),
        };
    }
    if action == "artifacts" {
        return match subaction {
            "" | "list" | "get" | "content" => Some("axon:read"),
            _ => Some("__deny__"),
        };
    }
    if action == "chat" && !subaction.is_empty() {
        return Some("__deny__");
    }
    if action == "jobs" {
        return match subaction {
            "list" | "get" | "status" | "events" | "stream" => Some("axon:read"),
            "cancel" | "retry" => Some("axon:write"),
            "recover" | "cleanup" | "clear" => Some("axon:admin"),
            _ => Some("__deny__"),
        };
    }
    // U2-20/C6-20: `memory search`/`memory show`/`memory context` are pure
    // retrieval and default to `axon:read`; every other memory subaction
    // (remember/link/supersede/forget/import/replace-scope/…) mutates state
    // and stays `axon:write` (or `axon:admin` for the replace-scope import,
    // enforced separately by `memory_authz` in `server.rs`).
    if action == "memory" {
        return match subaction {
            "search" | "show" | "context" => Some("axon:read"),
            _ => Some("axon:write"),
        };
    }
    // `watch` (issue #298 WS-B): per-subaction scope mirroring the REST
    // `/v1/watches` surface and `axon_services::action_api`'s
    // `AxonRequest::Watch` resolution. `list`/`get`/`history` are pure
    // retrieval; `create`/`exec`/`update`/`pause`/`resume`/`delete` mutate
    // state.
    if action == "watch" {
        return match subaction {
            "list" | "get" | "status" | "history" | "" => Some("axon:read"),
            "create" | "exec" | "update" | "pause" | "resume" | "delete" => Some("axon:write"),
            _ => Some("__deny__"),
        };
    }
    MCP_ACTION_SPECS
        .iter()
        .find(|spec| spec.name == action)
        .map_or(Some("__deny__"), |spec| spec.scope.as_scope(subaction))
}

#[cfg(test)]
#[path = "authz_tests.rs"]
mod tests;

pub(crate) fn required_scope_for_tool(
    tool_name: &str,
    action: &str,
    subaction: &str,
) -> Option<&'static str> {
    match tool_name {
        "axon_status_dashboard" => Some("axon:read"),
        _ => required_scope_for(action, subaction),
    }
}

/// Conditional scope upgrade (`mutates_if`, axon #298 follow-up).
///
/// `docs/pipeline-unification/surfaces/tool-contract.md`'s Auth and
/// Visibility table classifies `search`/`ask`/`research`/`summarize` as
/// `axon:read` query-shaped surfaces — and `required_scope_for` above (and
/// the tests locking it) intentionally keep reporting that nominal class, so
/// `axon:capabilities`/schema consumers still see the documented default.
/// But two of them do NOT have a non-mutating default form today: `search`
/// (`handle_search` in `handlers_query.rs` always calls
/// `axon_services::search_crawl::search_and_index_sources`, which unconditionally
/// enqueues one bounded Source job per result URL) and `research`
/// (`handle_research` always calls
/// `axon_services::search::synthesis::research_with_context`, same
/// unconditional source auto-index). Neither request DTO (`SearchRequest`,
/// `ResearchRequest` in `axon-api::action`) exposes an opt-out field, so
/// the predicate is unconditionally true for these two actions today. This
/// function is the dispatch-time authority actually consulted by
/// `call_tool`/`tasks.rs` — when it returns `Some`, the caller must upgrade
/// the effective required scope regardless of what `required_scope_for`
/// reports.
///
/// `ask`/`evaluate`/`suggest`/`summarize` are deliberately excluded: verified
/// against their current handlers/services (`query_svc::ask`,
/// `query_svc::evaluate` — whose `crawl_enqueue_outcomes` is always an empty
/// `Vec::new()` stub — `query_svc::suggest`, `summarize_svc::summarize`),
/// none of them enqueue a job in the current runtime, so there is nothing to
/// upgrade yet. Extend this predicate (ideally to inspect the parsed request
/// once a real per-call opt-out/opt-in option exists) if that changes.
///
/// **CWE-863 note:** callers MUST gate the scope this returns with
/// [`check_scope_explicit`], not [`check_scope`]. `check_scope` calls
/// `axon_authz::scope_satisfies`, which treats `axon:read` and `axon:write`
/// as interchangeable for ordinary broad routes — reusing it here would make
/// this whole elevation a silent no-op, since a caller holding only
/// `axon:read` already "satisfies" `axon:write` under that broad rule. See
/// `check_scope_explicit`'s doc comment.
pub fn mutates_if_upgrade(action: &str) -> Option<&'static str> {
    match action {
        "search" | "research" => Some("axon:write"),
        _ => None,
    }
}

/// Apply [`mutates_if_upgrade`] on top of a base required-scope lookup.
/// `__deny__`/`None` bases are left untouched — an upgrade is only applied
/// when the base lookup already resolved to a real scope requirement.
pub fn required_scope_with_mutates_if(
    action: &str,
    base: Option<&'static str>,
) -> Option<&'static str> {
    match base {
        Some("__deny__") | None => base,
        Some(_) => mutates_if_upgrade(action).or(base),
    }
}

/// Enforce the dispatch-time scope gate for a `tools/call`.
///
/// Extracted verbatim from `ServerHandler::call_tool` so that function stays
/// under the repo's 120-line monolith limit — the rmcp 3.0 migration pushed it
/// to 126. The logic is unchanged; only its location moved.
///
/// `auth` is `None` under `AuthPolicy::LoopbackDev`, which is locally-trusted
/// and enforces nothing.
pub(crate) fn enforce_call_tool_scope(
    auth: Option<&AuthContext>,
    tool_name: &str,
    action: &str,
    subaction: &str,
) -> Result<(), ErrorData> {
    // mutates_if (axon #298 follow-up): actions such as `search`/
    // `research` are documented as `axon:read` query surfaces but
    // unconditionally enqueue a background job today — upgrade the
    // dispatch-time requirement to `axon:write` regardless of what the
    // nominal action-class lookup reports. See
    // `mutates_if_upgrade` for the predicate and why only
    // these two actions are covered right now.
    let base_required_scope = required_scope_for_tool(tool_name, action, subaction);
    let required_scope = required_scope_with_mutates_if(action, base_required_scope);
    // CWE-863 fix: when `mutates_if_upgrade` actually elevated the
    // requirement (i.e. this action is `search`/`research`), gate with
    // `check_scope_explicit` instead of `check_scope`. `check_scope`
    // calls `axon_authz::scope_satisfies`, which deliberately treats
    // `axon:read`/`axon:write` as interchangeable for ordinary broad
    // routes — reusing it here made the elevation a silent no-op (a
    // caller holding only `axon:read` already "satisfied" `axon:write`).
    // See `check_scope_explicit`'s doc comment.
    let is_elevated = mutates_if_upgrade(action).is_some()
        || (action == "jobs" && matches!(subaction, "cancel" | "retry"));
    match (auth, required_scope) {
        // Deny: sentinel returned for unknown actions — even with a valid
        // token, we refuse rather than accidentally granting access.
        (Some(_), Some("__deny__")) => {
            tracing::warn!(
                action = %action,
                "MCP tool invocation denied: unknown action (fail-conservative)"
            );
            return Err(ErrorData::invalid_request(
                format!("forbidden: unknown action `{action}`"),
                None,
            ));
        }
        // No scope required (e.g. "help") — allowed through when authenticated.
        (Some(_), None) => {}
        // Scope check required.
        (Some(auth_ctx), Some(required_scope)) if is_elevated => {
            check_scope_explicit(auth_ctx, required_scope, action)?;
        }
        (Some(auth_ctx), Some(required_scope)) => {
            check_scope(auth_ctx, required_scope, action)?;
        }
        // LoopbackDev — no enforcement.
        (None, _) => {}
    }
    Ok(())
}
