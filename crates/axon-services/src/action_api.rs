mod commands;

use std::error::Error;

/// Collect, trim, and deduplicate URLs from optional singular and plural fields.
/// MCP handlers and action-API dispatchers share this inner logic; only the
/// error type differs, so callers do the empty check with their own error type.
pub fn collect_unique_urls(url: Option<String>, urls: Option<Vec<String>>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for u in urls
        .unwrap_or_default()
        .into_iter()
        .chain(url)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        if !out.contains(&u) {
            out.push(u);
        }
    }
    out
}

use crate::context::ServiceContext;
use crate::system;
use crate::types::ClientActionError;
use axon_api::mcp_schema::{
    AxonRequest, JobsSubaction, MemorySubaction, SetupMode, WatchSubaction,
};

pub async fn dispatch_action(
    service_context: &ServiceContext,
    action: AxonRequest,
) -> Result<serde_json::Value, ClientActionError> {
    match action {
        AxonRequest::Status(_) => {
            let result = system::full_status(service_context)
                .await
                .map_err(internal_error)?;
            Ok(result.payload)
        }
        AxonRequest::Extract(req) => commands::dispatch_extract(service_context, req).await,
        AxonRequest::Jobs(req) => commands::dispatch_jobs(service_context, req).await,
        // `/v1/actions` (this dispatcher's only caller) is removed from the
        // REST router (`v1_actions_removed`) — this arm has no live caller
        // and no auth context to derive scopes from, so it fails closed
        // rather than assuming admin.
        AxonRequest::Memory(req) => {
            crate::memory::dispatch(
                service_context,
                req,
                &crate::memory::MemoryAuthz::anonymous(),
            )
            .await
        }
        AxonRequest::Endpoints(req) => commands::dispatch_endpoints(service_context, req).await,
        AxonRequest::Summarize(req) => commands::dispatch_summarize(service_context, req).await,
        AxonRequest::Screenshot(req) => commands::dispatch_screenshot(service_context, req).await,
        AxonRequest::Diff(req) => commands::dispatch_diff(service_context, req).await,
        AxonRequest::Brand(req) => commands::dispatch_brand(service_context, req).await,
        other => Err(unsupported_action(action_name(&other))),
    }
}

pub fn required_scope(action: &AxonRequest) -> Option<&'static str> {
    match action {
        AxonRequest::Status(_) => Some("axon:read"),
        AxonRequest::Extract(_) => Some("axon:write"),
        AxonRequest::Memory(req) => match req.subaction.unwrap_or(MemorySubaction::Remember) {
            MemorySubaction::Remember
            | MemorySubaction::Link
            | MemorySubaction::Supersede
            | MemorySubaction::Reinforce
            | MemorySubaction::Contradict
            | MemorySubaction::Pin
            | MemorySubaction::Archive
            | MemorySubaction::Forget
            | MemorySubaction::Compact
            | MemorySubaction::Import => Some("axon:write"),
            MemorySubaction::List
            | MemorySubaction::Search
            | MemorySubaction::Show
            | MemorySubaction::Context
            | MemorySubaction::Review
            | MemorySubaction::Export => Some("axon:read"),
        },
        AxonRequest::Jobs(req) => match req.subaction.unwrap_or(JobsSubaction::List) {
            JobsSubaction::List
            | JobsSubaction::Get
            | JobsSubaction::Status
            | JobsSubaction::Events
            | JobsSubaction::Stream => Some("axon:read"),
            JobsSubaction::Cancel | JobsSubaction::Retry => Some("axon:write"),
            JobsSubaction::Recover | JobsSubaction::Cleanup | JobsSubaction::Clear => {
                Some("axon:admin")
            }
        },
        // Read-only ops: pure data reads, no external process, no side-effects.
        AxonRequest::Query(_)
        | AxonRequest::Retrieve(_)
        | AxonRequest::Search(_)
        | AxonRequest::Map(_)
        | AxonRequest::Doctor(_)
        | AxonRequest::Domains(_)
        | AxonRequest::Sources(_)
        | AxonRequest::Stats(_)
        | AxonRequest::Help(_)
        | AxonRequest::Chat(_) => Some("axon:read"),
        // These trigger Gemini headless completions (external process, API quota) — write scope.
        // Note: Debug runs LLM-assisted troubleshooting (Gemini) so it belongs here, not above.
        AxonRequest::Ask(_)
        | AxonRequest::Summarize(_)
        | AxonRequest::Evaluate(_)
        | AxonRequest::Suggest(_)
        | AxonRequest::Research(_)
        | AxonRequest::Debug(_) => Some("axon:write"),
        // Destructive / admin operations. INVARIANT: this must never return None here — the
        // authorize_action unconditional-auth guard for migrate depends on required_scope
        // returning Some(...) so the scope check runs after auth is confirmed.
        AxonRequest::Migrate(_) => Some("axon:write"),
        // Prune is admin-gated per the pruning contract: destructive prune
        // requires axon:admin, not just axon:write. The action-level scope
        // check here is the coarse "can call this action at all" gate;
        // axon_services::prune::prune's own PruneAuthz derivation is the
        // fine-grained "is this specific execution destructive" gate.
        AxonRequest::Prune(_) => Some("axon:admin"),
        AxonRequest::Watch(req) => match req.subaction.unwrap_or(WatchSubaction::List) {
            WatchSubaction::List
            | WatchSubaction::Get
            | WatchSubaction::Status
            | WatchSubaction::History => Some("axon:read"),
            WatchSubaction::Create
            | WatchSubaction::Exec
            | WatchSubaction::Update
            | WatchSubaction::Pause
            | WatchSubaction::Resume
            | WatchSubaction::Delete => Some("axon:write"),
        },
        AxonRequest::Setup(req) => match req.mode.unwrap_or(SetupMode::Check) {
            SetupMode::Check => Some("axon:read"),
            SetupMode::FirstRun | SetupMode::Repair | SetupMode::MigrateEnv => Some("axon:write"),
        },
        AxonRequest::Screenshot(_)
        | AxonRequest::Endpoints(_)
        | AxonRequest::Diff(_)
        | AxonRequest::Brand(_) => Some("axon:write"),
        AxonRequest::Source(_) => Some("axon:write"),
        // resolve/capabilities/providers (issue #298 WS-G): read-only
        // discovery surfaces, no side-effects.
        AxonRequest::Resolve(_) | AxonRequest::Capabilities(_) | AxonRequest::Providers(_) => {
            Some("axon:read")
        }
        // graph (issue #298 GQ): read-only SourceGraph query surface. Every
        // subaction (kinds/resolve/query/node/edge/source) is a pure read —
        // graph writes stay parser/source-job owned.
        AxonRequest::Graph(_) => Some("axon:read"), // NOTE: no wildcard arm — the match must be exhaustive.
                                                    // Adding a new AxonRequest variant without a required_scope arm is a compile error,
                                                    // which is the correct enforcement mechanism: scope assignment is opt-out, not opt-in.
    }
}

fn unsupported_action(action: &'static str) -> ClientActionError {
    ClientActionError::new(
        "unsupported_action",
        format!("{action} is not supported by the first-party action API yet"),
        false,
        Some("call /v1/capabilities to discover supported actions".to_string()),
    )
}

fn internal_error(err: Box<dyn Error>) -> ClientActionError {
    ClientActionError::new("internal", err.to_string(), true, None)
}

fn action_name(action: &AxonRequest) -> &'static str {
    match action {
        AxonRequest::Status(_) => "status",
        AxonRequest::Jobs(_) => "jobs",
        AxonRequest::Extract(_) => "extract",
        AxonRequest::Memory(_) => "memory",
        AxonRequest::Query(_) => "query",
        AxonRequest::Retrieve(_) => "retrieve",
        AxonRequest::Search(_) => "search",
        AxonRequest::Map(_) => "map",
        AxonRequest::Endpoints(_) => "endpoints",
        AxonRequest::Evaluate(_) => "evaluate",
        AxonRequest::Suggest(_) => "suggest",
        AxonRequest::Doctor(_) => "doctor",
        AxonRequest::Domains(_) => "domains",
        AxonRequest::Sources(_) => "sources",
        AxonRequest::Stats(_) => "stats",
        AxonRequest::Help(_) => "help",
        AxonRequest::Research(_) => "research",
        AxonRequest::Ask(_) => "ask",
        AxonRequest::Summarize(_) => "summarize",
        AxonRequest::Screenshot(_) => "screenshot",
        AxonRequest::Brand(_) => "brand",
        AxonRequest::Debug(_) => "debug",
        AxonRequest::Diff(_) => "diff",
        AxonRequest::Prune(_) => "prune",
        AxonRequest::Migrate(_) => "migrate",
        AxonRequest::Watch(_) => "watch",
        AxonRequest::Setup(_) => "setup",
        AxonRequest::Source(_) => "source",
        AxonRequest::Resolve(_) => "resolve",
        AxonRequest::Capabilities(_) => "capabilities",
        AxonRequest::Providers(_) => "providers",
        AxonRequest::Graph(_) => "graph",
        AxonRequest::Chat(_) => "chat",
    }
}
