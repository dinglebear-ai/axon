use super::super::auth::AuthPolicy;
use lab_auth::AuthContext;
use rmcp::RoleServer;
use rmcp::model::ErrorData;
use rmcp::service::RequestContext;

/// Extract and enforce the authentication context from the rmcp request.
///
/// - `AuthPolicy::LoopbackDev`: always returns `Ok(None)` — the loopback bind
///   is the trust boundary; no per-request credential needed.
/// - `AuthPolicy::Mounted(_)`: the middleware MUST have inserted an
///   [`AuthContext`] into the request extensions. If it is absent, this
///   returns a forbidden error immediately (fail-closed).
///
/// Returns `Ok(Some(&AuthContext))` for Mounted+present, `Ok(None)` for
/// LoopbackDev.
pub fn require_auth_context<'a>(
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
                    // Framework-level invariant violation: rmcp changed how it
                    // propagates HTTP Parts, or middleware ordering is broken.
                    tracing::error!(
                        "rmcp HTTP Parts extension absent — middleware ordering may be broken"
                    );
                    ErrorData::invalid_request("forbidden: missing http context", None)
                })?;
            let auth = parts.extensions.get::<AuthContext>().ok_or_else(|| {
                // AuthLayer should always insert AuthContext on the happy path.
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
/// `axon:write` is treated as a superset of `axon:read` — a caller with write
/// access implicitly satisfies any read-level scope requirement.
pub fn check_scope(
    auth: &AuthContext,
    required_scope: &str,
    action: &str,
) -> Result<(), ErrorData> {
    let satisfied = auth
        .scopes
        .iter()
        .any(|s| s == required_scope || (required_scope == "axon:read" && s == "axon:write"));
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

/// Map an axon tool action (and optional sub-action) to the minimum required scope.
///
/// Returns `None` for informational actions that need `AuthContext` (when
/// Mounted) but no specific scope gate — e.g. `help`.
/// Unknown actions return `Some("__deny__")` — a sentinel that `call_tool`
/// treats as an explicit deny rather than skipping the check. This is
/// fail-conservative: new actions added without a mapping entry are denied
/// rather than accidentally permitted.
///
/// Note on `"artifacts"`: sub-actions `delete` and `clean` require
/// `axon:write`; all others require `axon:read`. The caller passes the
/// `subaction` field from the parsed request arguments.
///
/// Note on `"scrape"`: scrape crawls and stores content — it is a write
/// operation and requires `axon:write`.
pub fn required_scope_for(action: &str, subaction: &str) -> Option<&'static str> {
    match action {
        // Informational — AuthContext required when Mounted, but no scope gate.
        "help" => None,
        // Write/mutating operations require axon:write.
        "crawl" | "extract" | "embed" | "ingest" | "scrape" => Some("axon:write"),
        // artifacts: write subactions need axon:write, read subactions need axon:read.
        "artifacts" => match subaction {
            "delete" | "clean" => Some("axon:write"),
            _ => Some("axon:read"),
        },
        // Read / query operations require axon:read.
        "status" | "query" | "retrieve" | "search" | "map" | "evaluate" | "suggest" | "doctor"
        | "domains" | "sources" | "stats" | "research" | "ask" | "screenshot" => Some("axon:read"),
        // Unknown actions are explicitly denied (fail-conservative). Add an
        // explicit mapping above for any new action before shipping.
        _ => Some("__deny__"),
    }
}
