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
use axon_api::action::{ActionRequest, JobsSubaction, MemorySubaction, SetupMode, WatchSubaction};

pub async fn dispatch_action(
    service_context: &ServiceContext,
    action: ActionRequest,
) -> Result<serde_json::Value, ClientActionError> {
    match action {
        ActionRequest::Status(_) => {
            let result = system::full_status(service_context)
                .await
                .map_err(internal_error)?;
            Ok(result.payload)
        }
        ActionRequest::Extract(req) => commands::dispatch_extract(service_context, req).await,
        ActionRequest::Jobs(req) => commands::dispatch_jobs(service_context, req).await,
        // `/v1/actions` (this dispatcher's only caller) is removed from the
        // REST router (`v1_actions_removed`) — this arm has no live caller
        // and no auth context to derive scopes from, so it fails closed
        // rather than assuming admin.
        ActionRequest::Memory(req) => {
            crate::memory::dispatch(
                service_context,
                req,
                &crate::memory::MemoryAuthz::anonymous(),
            )
            .await
        }
        ActionRequest::Endpoints(req) => commands::dispatch_endpoints(service_context, req).await,
        ActionRequest::Summarize(req) => commands::dispatch_summarize(service_context, req).await,
        ActionRequest::Screenshot(req) => commands::dispatch_screenshot(service_context, req).await,
        ActionRequest::Diff(req) => commands::dispatch_diff(service_context, req).await,
        ActionRequest::Brand(req) => commands::dispatch_brand(service_context, req).await,
        other => Err(unsupported_action(action_name(&other))),
    }
}

pub fn required_scope(action: &ActionRequest) -> Option<&'static str> {
    match action {
        ActionRequest::Scrape(_)
        | ActionRequest::Crawl(_)
        | ActionRequest::Embed(_)
        | ActionRequest::Ingest(_) => Some("axon:write"),
        ActionRequest::CodeSearch(_) => Some("axon:read"),
        ActionRequest::Status(_) => Some("axon:read"),
        ActionRequest::Extract(_) => Some("axon:write"),
        ActionRequest::Memory(req) => match req.subaction.unwrap_or(MemorySubaction::Remember) {
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
        ActionRequest::Jobs(req) => match req.subaction.unwrap_or(JobsSubaction::List) {
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
        ActionRequest::Query(_)
        | ActionRequest::Retrieve(_)
        | ActionRequest::Search(_)
        | ActionRequest::Map(_)
        | ActionRequest::Doctor(_)
        | ActionRequest::Domains(_)
        | ActionRequest::Sources(_)
        | ActionRequest::Stats(_)
        | ActionRequest::Help(_)
        | ActionRequest::Chat(_) => Some("axon:read"),
        // These trigger Gemini headless completions (external process, API quota) — write scope.
        // Note: Debug runs LLM-assisted troubleshooting (Gemini) so it belongs here, not above.
        ActionRequest::Ask(_)
        | ActionRequest::Summarize(_)
        | ActionRequest::Evaluate(_)
        | ActionRequest::Suggest(_)
        | ActionRequest::Research(_)
        | ActionRequest::Debug(_) => Some("axon:write"),
        // Destructive / admin operations. INVARIANT: this must never return None here — the
        // authorize_action unconditional-auth guard for migrate depends on required_scope
        // returning Some(...) so the scope check runs after auth is confirmed.
        ActionRequest::Migrate(_) => Some("axon:write"),
        // Prune is admin-gated per the pruning contract: destructive prune
        // requires axon:admin, not just axon:write. The action-level scope
        // check here is the coarse "can call this action at all" gate;
        // axon_services::prune::prune's own PruneAuthz derivation is the
        // fine-grained "is this specific execution destructive" gate.
        ActionRequest::Prune(_) => Some("axon:admin"),
        ActionRequest::Watch(req) => match req.subaction.unwrap_or(WatchSubaction::List) {
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
        ActionRequest::Setup(req) => match req.mode.unwrap_or(SetupMode::Check) {
            SetupMode::Check => Some("axon:read"),
            SetupMode::FirstRun | SetupMode::Repair | SetupMode::MigrateEnv => Some("axon:write"),
        },
        ActionRequest::Screenshot(_)
        | ActionRequest::Endpoints(_)
        | ActionRequest::Diff(_)
        | ActionRequest::Brand(_) => Some("axon:write"),
        ActionRequest::Source(_) => Some("axon:write"),
        // resolve/capabilities/providers (issue #298 WS-G): read-only
        // discovery surfaces, no side-effects.
        ActionRequest::Resolve(_)
        | ActionRequest::Capabilities(_)
        | ActionRequest::Providers(_) => Some("axon:read"),
        // graph (issue #298 GQ): read-only SourceGraph query surface. Every
        // subaction (kinds/resolve/query/node/edge/source) is a pure read —
        // graph writes stay parser/source-job owned.
        ActionRequest::Graph(_) => Some("axon:read"), // NOTE: no wildcard arm — the match must be exhaustive.
        ActionRequest::Codex(_) => Some("axon:admin"),
        // Adding a new ActionRequest variant without a required_scope arm is a compile error,
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

fn action_name(action: &ActionRequest) -> &'static str {
    match action {
        ActionRequest::Scrape(_) => "scrape",
        ActionRequest::Crawl(_) => "crawl",
        ActionRequest::Embed(_) => "embed",
        ActionRequest::Ingest(_) => "ingest",
        ActionRequest::CodeSearch(_) => "code_search",
        ActionRequest::Status(_) => "status",
        ActionRequest::Jobs(_) => "jobs",
        ActionRequest::Extract(_) => "extract",
        ActionRequest::Memory(_) => "memory",
        ActionRequest::Query(_) => "query",
        ActionRequest::Retrieve(_) => "retrieve",
        ActionRequest::Search(_) => "search",
        ActionRequest::Map(_) => "map",
        ActionRequest::Endpoints(_) => "endpoints",
        ActionRequest::Evaluate(_) => "evaluate",
        ActionRequest::Suggest(_) => "suggest",
        ActionRequest::Doctor(_) => "doctor",
        ActionRequest::Domains(_) => "domains",
        ActionRequest::Sources(_) => "sources",
        ActionRequest::Stats(_) => "stats",
        ActionRequest::Help(_) => "help",
        ActionRequest::Research(_) => "research",
        ActionRequest::Ask(_) => "ask",
        ActionRequest::Summarize(_) => "summarize",
        ActionRequest::Screenshot(_) => "screenshot",
        ActionRequest::Brand(_) => "brand",
        ActionRequest::Debug(_) => "debug",
        ActionRequest::Diff(_) => "diff",
        ActionRequest::Prune(_) => "prune",
        ActionRequest::Migrate(_) => "migrate",
        ActionRequest::Watch(_) => "watch",
        ActionRequest::Setup(_) => "setup",
        ActionRequest::Source(_) => "source",
        ActionRequest::Resolve(_) => "resolve",
        ActionRequest::Capabilities(_) => "capabilities",
        ActionRequest::Providers(_) => "providers",
        ActionRequest::Graph(_) => "graph",
        ActionRequest::Chat(_) => "chat",
        ActionRequest::Codex(_) => "codex",
    }
}
