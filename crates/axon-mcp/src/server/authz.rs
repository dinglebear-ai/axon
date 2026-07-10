use crate::auth::AuthPolicy;
use axon_authz::scope_satisfies;
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
        name: "sources",
        scope: ActionScope::Read,
        description: "List indexed URLs and chunk counts",
        cost: "cheap",
    },
    McpActionSpec {
        name: "domains",
        scope: ActionScope::Read,
        description: "List indexed domains and aggregate stats",
        cost: "cheap",
    },
    McpActionSpec {
        name: "stats",
        scope: ActionScope::Read,
        description: "Show Qdrant collection statistics",
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
    McpActionSpec {
        name: "search",
        scope: ActionScope::Read,
        description: "Run SearXNG/Tavily web search and optionally queue crawls for results",
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
        description: "Plan (dry-run) or execute (destructive) a prune of a source, generation, or collection",
        cost: "write",
    },
    // U2-20/C6-20: ask/evaluate/suggest/research/summarize default to
    // `axon:read` — they're query-shaped surfaces, even though research (and
    // occasionally ask/summarize) may enqueue a background crawl/index job as
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
        description: "Suggest new documentation URLs to crawl",
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
        description: "Run or manage async structured extraction jobs",
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
        description: "Scrape URL context and summarize it with the configured LLM",
        cost: "write",
    },
    McpActionSpec {
        name: "endpoints",
        scope: ActionScope::Write,
        description: "Discover and optionally verify static site endpoints",
        cost: "write",
    },
    McpActionSpec {
        name: "elicit_demo",
        scope: ActionScope::Write,
        description: "Exercise MCP elicitation support with a demo form",
        cost: "write",
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

/// Map an axon tool action and subaction to the minimum required scope.
pub fn required_scope_for(action: &str, subaction: &str) -> Option<&'static str> {
    if action == "jobs" {
        return match subaction {
            "list" | "get" | "status" | "events" | "stream" | "artifacts" => Some("axon:read"),
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
    MCP_ACTION_SPECS
        .iter()
        .find(|spec| spec.name == action)
        .map_or(Some("__deny__"), |spec| spec.scope.as_scope(subaction))
}

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
