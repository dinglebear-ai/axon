//! `ExtractService` — structured extraction, summarize, and research.
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §ExtractService. `summarize`/`research` wrap real free functions
//! (`crate::summarize::summarize`, `crate::search::research_with_context_tracked`);
//! `summarize` mirrors the MCP `handle_summarize` handler
//! (`crates/axon-mcp/src/server/handlers_query.rs`) exactly: URLs are
//! collected via `crate::action_api::collect_unique_urls` and
//! `render_mode`/`root_selector`/`exclude_selector` are applied onto the
//! config via `ConfigOverrides` before calling `crate::summarize::summarize`,
//! so no `SummarizeRequest` field is silently dropped.
//! `extract` is SKIP — `extract/sync.rs::extract_sync`'s exact signature vs.
//! the contract's `ExtractRequest`/`ExtractResult` shape was not confirmed in
//! this pass, and no `axon-api::ExtractResult` DTO exists (only the
//! job-lifecycle-shaped `ExtractRequest` in `mcp_schema::requests`) — see the
//! approved wiring plan's deferred list.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::mcp_schema::ExtractRequest;
use axon_api::mcp_schema::McpRenderMode;
use axon_api::mcp_schema::{ResearchRequest, SummarizeRequest};
use axon_core::config::{ConfigOverrides, RenderMode};

use crate::context::ServiceContext;
use crate::service_traits::not_implemented;
use crate::types::{ResearchResult, SummarizeResult};

/// Local copy of `axon_services::action_api::commands::helpers::map_render_mode`
/// (that helper is `pub(super)`-scoped to `action_api::commands`). Trivial,
/// exhaustive wire-enum mapping — safe to duplicate here rather than widen
/// that module's visibility.
fn map_render_mode(mode: McpRenderMode) -> RenderMode {
    match mode {
        McpRenderMode::Http => RenderMode::Http,
        McpRenderMode::Chrome => RenderMode::Chrome,
        McpRenderMode::AutoSwitch => RenderMode::AutoSwitch,
    }
}

/// Deferred per the module doc comment: no `axon-api::ExtractResult` DTO
/// exists yet and `extract_sync`'s exact signature was not confirmed against
/// the contract's `ExtractRequest` shape in this pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractResult {
    pub urls: Vec<String>,
    pub extracted: Vec<serde_json::Value>,
}

#[async_trait]
pub trait ExtractService: Send + Sync {
    async fn extract(&self, request: ExtractRequest) -> anyhow::Result<ExtractResult>;
    async fn summarize(&self, request: SummarizeRequest) -> anyhow::Result<SummarizeResult>;
    async fn research(&self, request: ResearchRequest) -> anyhow::Result<ResearchResult>;
}

pub struct ExtractServiceImpl {
    ctx: Arc<ServiceContext>,
}

impl ExtractServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ExtractService for ExtractServiceImpl {
    async fn extract(&self, _request: ExtractRequest) -> anyhow::Result<ExtractResult> {
        Err(not_implemented("ExtractService::extract"))
    }

    async fn summarize(&self, request: SummarizeRequest) -> anyhow::Result<SummarizeResult> {
        let urls = crate::action_api::collect_unique_urls(request.url, request.urls);
        let cfg = self.ctx.cfg().apply_overrides(&ConfigOverrides {
            render_mode: request.render_mode.map(map_render_mode),
            root_selector: request.root_selector,
            exclude_selector: request.exclude_selector,
            ..ConfigOverrides::default()
        });
        crate::summarize::summarize(&cfg, &urls, None)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn research(&self, _request: ResearchRequest) -> anyhow::Result<ResearchResult> {
        // `crate::search::research_with_context_tracked`'s future is not
        // `Send`: it transitively calls `job_tracking::track_research_job`,
        // which returns `Result<ResearchResult, Box<dyn Error>>` and the
        // non-`Send` `Box<dyn Error>` gets captured across an `.await` point
        // inside that async fn body. This conflicts with `#[async_trait]`'s
        // default `Send`-bound futures (verified directly: swapping in the
        // real call fails `cargo check -p axon-services` with E0277 "`dyn
        // StdError` cannot be sent between threads safely", anchored at
        // `crates/axon-services/src/search/job_tracking.rs:38`). Fixing that
        // is pre-existing `axon-services` work, not a thin wrap — stubbed
        // pending a follow-up (either `#[async_trait(?Send)]` on this trait
        // or making the free function's error type `Send`).
        Err(not_implemented("ExtractService::research"))
    }
}

/// Deterministic in-memory fake covering every `ExtractService` method.
#[derive(Default)]
pub struct FakeExtractService {
    calls: Mutex<Vec<String>>,
}

impl FakeExtractService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[async_trait]
impl ExtractService for FakeExtractService {
    async fn extract(&self, request: ExtractRequest) -> anyhow::Result<ExtractResult> {
        self.calls.lock().unwrap().push("extract".to_string());
        Ok(ExtractResult {
            urls: request.urls.unwrap_or_default(),
            extracted: Vec::new(),
        })
    }

    async fn summarize(&self, request: SummarizeRequest) -> anyhow::Result<SummarizeResult> {
        self.calls.lock().unwrap().push("summarize".to_string());
        let urls: Vec<String> = match (request.url, request.urls) {
            (_, Some(urls)) if !urls.is_empty() => urls,
            (Some(url), _) => vec![url],
            _ => Vec::new(),
        };
        Ok(SummarizeResult {
            urls: urls.clone(),
            documents: Vec::new(),
            summary: "fake summary".to_string(),
            context_chars: 0,
            context_truncated: false,
            usage: None,
            timing_ms: crate::types::SummarizeTiming {
                scrape: 0,
                llm: 0,
                total: 0,
            },
        })
    }

    async fn research(&self, request: ResearchRequest) -> anyhow::Result<ResearchResult> {
        self.calls.lock().unwrap().push("research".to_string());
        Ok(ResearchResult {
            payload: crate::types::ResearchPayload {
                query: request.query.unwrap_or_default(),
                limit: request.limit.unwrap_or(10),
                offset: request.offset.unwrap_or(0),
                search_results: Vec::new(),
                extractions: Vec::new(),
                auto_crawl_status: "skipped".to_string(),
                crawl_jobs: Vec::new(),
                crawl_jobs_rejected: Vec::new(),
                summary: Some("fake research summary".to_string()),
                summary_source: crate::types::SummarySource::None,
                usage: crate::types::ResearchUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                },
                timing_ms: crate::types::ResearchTiming { total: 0 },
            },
        })
    }
}

#[cfg(test)]
#[path = "extract_service_tests.rs"]
mod tests;
