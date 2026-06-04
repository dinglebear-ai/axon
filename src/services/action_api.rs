mod commands;

use std::error::Error;

/// Collect, trim, and deduplicate URLs from optional singular and plural fields.
/// MCP handlers and action-API dispatchers share this inner logic; only the
/// error type differs, so callers do the empty check with their own error type.
pub(crate) fn collect_unique_urls(url: Option<String>, urls: Option<Vec<String>>) -> Vec<String> {
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

use crate::mcp::schema::{
    AxonRequest, CrawlSubaction, EmbedSubaction, ExtractSubaction, IngestSubaction, SetupMode,
    WatchSubaction,
};
use crate::services::context::ServiceContext;
use crate::services::system;
use crate::services::types::ClientActionError;

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
        AxonRequest::Crawl(req) => commands::dispatch_crawl(service_context, req).await,
        AxonRequest::Extract(req) => commands::dispatch_extract(service_context, req).await,
        AxonRequest::Embed(req) => commands::dispatch_embed(service_context, req).await,
        AxonRequest::Ingest(req) => commands::dispatch_ingest(service_context, req).await,
        AxonRequest::Endpoints(req) => commands::dispatch_endpoints(service_context, req).await,
        AxonRequest::Scrape(req) => commands::dispatch_scrape(service_context, req).await,
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
        AxonRequest::Crawl(req) => match req.subaction.unwrap_or(CrawlSubaction::Start) {
            CrawlSubaction::Status | CrawlSubaction::List => Some("axon:read"),
            CrawlSubaction::Start
            | CrawlSubaction::Cancel
            | CrawlSubaction::Cleanup
            | CrawlSubaction::Clear
            | CrawlSubaction::Recover => Some("axon:write"),
        },
        AxonRequest::Extract(req) => match req.subaction.unwrap_or(ExtractSubaction::Start) {
            ExtractSubaction::Status | ExtractSubaction::List => Some("axon:read"),
            _ => Some("axon:write"),
        },
        AxonRequest::Embed(req) => match req.subaction.unwrap_or(EmbedSubaction::Start) {
            EmbedSubaction::Status | EmbedSubaction::List => Some("axon:read"),
            _ => Some("axon:write"),
        },
        AxonRequest::Ingest(req) => match req.subaction.unwrap_or(IngestSubaction::Start) {
            IngestSubaction::Status | IngestSubaction::List => Some("axon:read"),
            _ => Some("axon:write"),
        },
        // Read-only ops: pure data reads, no external process, no side-effects.
        AxonRequest::Query(_)
        | AxonRequest::Retrieve(_)
        | AxonRequest::Search(_)
        | AxonRequest::Map(_)
        | AxonRequest::Endpoints(_)
        | AxonRequest::Doctor(_)
        | AxonRequest::Domains(_)
        | AxonRequest::Sources(_)
        | AxonRequest::Stats(_)
        | AxonRequest::Help(_) => Some("axon:read"),
        // These trigger Gemini headless completions (external process, API quota) — write scope.
        // Note: Debug runs LLM-assisted troubleshooting (Gemini) so it belongs here, not above.
        AxonRequest::Ask(_)
        | AxonRequest::Summarize(_)
        | AxonRequest::Evaluate(_)
        | AxonRequest::Suggest(_)
        | AxonRequest::Research(_)
        | AxonRequest::Debug(_) => Some("axon:write"),
        // Destructive / admin operations. INVARIANT: these must never return None here — the
        // authorize_action unconditional-auth guard for Migrate/Dedupe silently depends on
        // required_scope returning Some(...) so the scope check runs after auth is confirmed.
        AxonRequest::Dedupe(_) | AxonRequest::Migrate(_) => Some("axon:write"),
        // ElicitDemo is an MCP elicitation primitive. Explicit arm prevents it silently
        // absorbing a future wildcard default change.
        AxonRequest::ElicitDemo(_) => Some("axon:write"),
        AxonRequest::Watch(req) => match req.subaction.unwrap_or(WatchSubaction::List) {
            WatchSubaction::List | WatchSubaction::Get | WatchSubaction::History => {
                Some("axon:read")
            }
            WatchSubaction::Create | WatchSubaction::RunNow => Some("axon:write"),
        },
        AxonRequest::Setup(req) => match req.mode.unwrap_or(SetupMode::Check) {
            SetupMode::Check => Some("axon:read"),
            SetupMode::FirstRun | SetupMode::Repair | SetupMode::MigrateEnv => Some("axon:write"),
        },
        AxonRequest::Scrape(_) | AxonRequest::Screenshot(_) => Some("axon:write"),
        AxonRequest::VerticalScrape(_) => Some("axon:write"),
        AxonRequest::Diff(_) | AxonRequest::Brand(_) => Some("axon:read"),
        // NOTE: no wildcard arm — the match must be exhaustive.
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
        AxonRequest::Crawl(_) => "crawl",
        AxonRequest::Extract(_) => "extract",
        AxonRequest::Embed(_) => "embed",
        AxonRequest::Ingest(_) => "ingest",
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
        AxonRequest::Scrape(_) => "scrape",
        AxonRequest::Research(_) => "research",
        AxonRequest::Ask(_) => "ask",
        AxonRequest::Summarize(_) => "summarize",
        AxonRequest::Screenshot(_) => "screenshot",
        AxonRequest::Brand(_) => "brand",
        AxonRequest::Debug(_) => "debug",
        AxonRequest::Diff(_) => "diff",
        AxonRequest::Dedupe(_) => "dedupe",
        AxonRequest::Migrate(_) => "migrate",
        AxonRequest::Watch(_) => "watch",
        AxonRequest::Setup(_) => "setup",
        AxonRequest::ElicitDemo(_) => "elicit_demo",
        AxonRequest::VerticalScrape(_) => "vertical_scrape",
    }
}

#[cfg(test)]
#[path = "action_api_tests.rs"]
mod tests;
