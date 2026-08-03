use crate::auth::AuthPolicy;
use axon_authz::{has_explicit_scope, scope_satisfies};
use lab_auth::AuthContext;
use rmcp::{ErrorData, RoleServer, service::RequestContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ActionScope {
    Read,
    Write,
    /// Destructive/admin-gated action. Per the auth contract, `axon:write`
    /// does NOT imply `axon:admin` — the caller must hold the fine-grained
    /// scope explicitly.
    Admin,
    InfoOnly,
}

impl ActionScope {
    pub(super) fn as_scope(self, _subaction: &str) -> Option<&'static str> {
        match self {
            Self::Read => Some("axon:read"),
            Self::Write => Some("axon:write"),
            Self::Admin => Some("axon:admin"),
            Self::InfoOnly => None,
        }
    }

    pub(super) fn as_label(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Admin => "admin",
            Self::InfoOnly => "info",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct McpActionSpec {
    pub name: &'static str,
    pub scope: ActionScope,
    pub description: &'static str,
    pub cost: &'static str,
}

pub(super) const MCP_ACTION_SPECS: &[McpActionSpec] = &[
    McpActionSpec {
        name: "help",
        scope: ActionScope::InfoOnly,
        description: "List actions, subactions, defaults, and schema resource links",
        cost: "cheap",
    },
    McpActionSpec {
        name: "status",
        scope: ActionScope::Read,
        description: "Show job queue, worker, and service status",
        cost: "cheap",
    },
    McpActionSpec {
        name: "jobs",
        scope: ActionScope::Write,
        description: "List, inspect, page events, cancel, retry, recover, cleanup, or clear unified durable jobs",
        cost: "write",
    },
    McpActionSpec {
        name: "doctor",
        scope: ActionScope::Read,
        description: "Diagnose Axon service connectivity",
        cost: "cheap",
    },
    McpActionSpec {
        name: "source",
        scope: ActionScope::Write,
        description: "Acquire and index one source (local path, git/web/feed/youtube/reddit/session/registry target) through the unified pipeline",
        cost: "write",
    },
    McpActionSpec {
        name: "query",
        scope: ActionScope::Read,
        description: "Run semantic vector search over indexed content",
        cost: "cheap",
    },
    McpActionSpec {
        name: "retrieve",
        scope: ActionScope::Read,
        description: "Fetch stored document chunks by URL",
        cost: "cheap",
    },
    // resolve/capabilities/providers (contract's `resolve`/`capabilities`/
    // `providers` actions, WS-G #298): read-only discovery surfaces backed by
    // real data — `resolve` calls `axon_services::source::routing::
    // resolve_source_route`, `providers` reshapes `system::doctor`'s per-
    // service payload (mirroring the REST `/v1/providers` resource-tier
    // routes), and `capabilities` reports the live `MCP_ACTION_SPECS`
    // registry plus provider health. None of these mutate state.
    McpActionSpec {
        name: "resolve",
        scope: ActionScope::Read,
        description: "Resolve source identity and adapter route without acquiring content",
        cost: "cheap",
    },
    McpActionSpec {
        name: "capabilities",
        scope: ActionScope::Read,
        description: "Machine-readable server capability document: actions, scopes, providers",
        cost: "cheap",
    },
    McpActionSpec {
        name: "providers",
        scope: ActionScope::Read,
        description: "List or inspect provider capability/health (list|get subactions)",
        cost: "cheap",
    },
    McpActionSpec {
        name: "search",
        scope: ActionScope::Read,
        description: "Run SearXNG/Tavily web search and optionally queue source auto-index jobs for results",
        cost: "moderate",
    },
    McpActionSpec {
        name: "map",
        scope: ActionScope::Read,
        description: "Discover URLs for a site without scraping page content",
        cost: "moderate",
    },
    McpActionSpec {
        name: "prune",
        scope: ActionScope::Admin,
        description: "Plan or execute source, generation, or collection cleanup behind axon-prune",
        cost: "write",
    },
    McpActionSpec {
        name: "collections",
        scope: ActionScope::Read,
        description: "List or inspect configured vector collections",
        cost: "cheap",
    },
    McpActionSpec {
        name: "reset",
        scope: ActionScope::Admin,
        description: "Plan or execute an explicit clean-slate store reset",
        cost: "write",
    },
    // U2-20/C6-20: ask/evaluate/suggest/research/summarize default to
    // `axon:read` — they're query-shaped surfaces, even though research (and
    // occasionally ask/summarize) may enqueue a background source/index job as
    // a side effect. No `mutates_if`/conditional-upgrade metadata exists yet
    // (tracked as a follow-up); until it lands these stay read-gated rather
    // than write-gated, matching the contract's stated default.
    McpActionSpec {
        name: "ask",
        scope: ActionScope::Read,
        description: "Answer a question with RAG over indexed content",
        cost: "moderate",
    },
    McpActionSpec {
        name: "evaluate",
        scope: ActionScope::Read,
        description: "Evaluate RAG quality against a baseline and judge diagnostics",
        cost: "expensive",
    },
    McpActionSpec {
        name: "suggest",
        scope: ActionScope::Read,
        description: "Suggest new documentation URLs to index",
        cost: "moderate",
    },
    McpActionSpec {
        name: "research",
        scope: ActionScope::Read,
        description: "Run SearXNG/Tavily research with synthesis and auto-indexing",
        cost: "expensive",
    },
    McpActionSpec {
        name: "screenshot",
        scope: ActionScope::Write,
        description: "Capture a full-page screenshot through headless Chrome",
        cost: "moderate",
    },
    McpActionSpec {
        name: "brand",
        scope: ActionScope::Write,
        description: "Extract brand identity metadata from a URL",
        cost: "write",
    },
    McpActionSpec {
        name: "diff",
        scope: ActionScope::Write,
        description: "Compare two URLs for content, metadata, and link changes",
        cost: "write",
    },
    McpActionSpec {
        name: "extract",
        scope: ActionScope::Write,
        description: "Start async structured extraction jobs; use action=jobs for lifecycle",
        cost: "write",
    },
    McpActionSpec {
        name: "memory",
        scope: ActionScope::Write,
        description: "Remember, search, and show persistent agent memory",
        cost: "write",
    },
    McpActionSpec {
        name: "summarize",
        scope: ActionScope::Read,
        description: "Fetch URL context and summarize it with the configured LLM",
        cost: "write",
    },
    McpActionSpec {
        name: "endpoints",
        scope: ActionScope::Write,
        description: "Discover and optionally verify static site endpoints",
        cost: "write",
    },
    // `watch` (issue #298 WS-B): source-request-backed watch subactions mirror
    // the REST `/v1/watches` surface. Per-subaction scope is enforced in
    // `required_scope_for` below (`list`/`get`/`history` read; mutating
    // lifecycle operations write).
    McpActionSpec {
        name: "watch",
        scope: ActionScope::Write,
        description: "Create, list, inspect, update, pause, resume, or delete source-request-backed watches",
        cost: "write",
    },
    // `graph` (issue #298 GQ): read-only SourceGraph query surface mirroring
    // the REST `/v1/graph/*` routes. All subactions (`kinds`/`resolve`/
    // `query`/`node`/`edge`/`source`) are pure reads — graph writes stay
    // parser/source-job owned, never caller-provided through this action.
    McpActionSpec {
        name: "graph",
        scope: ActionScope::Read,
        description: "Query the read-only SourceGraph: kinds, resolve, query, node, edge, source subgraph",
        cost: "cheap",
    },
    McpActionSpec {
        name: "uploads",
        scope: ActionScope::Write,
        description: "Stage, inspect, complete, list, or abort durable uploads",
        cost: "write",
    },
    McpActionSpec {
        name: "artifacts",
        scope: ActionScope::Read,
        description: "List, inspect, or read artifacts by opaque artifact id",
        cost: "cheap",
    },
    McpActionSpec {
        name: "chat",
        scope: ActionScope::Read,
        description: "Send a direct prompt to the configured chat-purpose LLM",
        cost: "moderate",
    },
];

pub(super) fn mcp_action_names() -> Vec<&'static str> {
    MCP_ACTION_SPECS.iter().map(|spec| spec.name).collect()
}

/// Extract and enforce the authentication context from the rmcp request.
///
/// `LoopbackDev` trusts process isolation. Mounted HTTP mode requires the auth
/// middleware to have inserted an `AuthContext` into request extensions.
pub(super) fn require_auth_context<'a>(
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
pub(super) fn check_scope(
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
/// `docs/reference/runtime/security.md`'s "Contract"
/// paragraph and `docs/reference/runtime/auth.md`'s
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
pub(super) fn check_scope_explicit(
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
            "" | "plan" | "exec" => Some("axon:admin"),
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
            "list" | "get" | "history" | "" => Some("axon:read"),
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

pub(super) fn required_scope_for_tool(
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
/// `docs/reference/mcp/tool-contract.md`'s Auth and
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
/// `ResearchRequest` in `axon-api::mcp_schema`) exposes an opt-out field, so
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
pub(super) fn enforce_call_tool_scope(
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
    let is_elevated = mutates_if_upgrade(action).is_some();
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
