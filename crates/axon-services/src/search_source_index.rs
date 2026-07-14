//! Source-backed auto-index enqueue helpers for search and research.
//!
//! Search/research result URLs are untrusted third-party output, so this path
//! builds a fresh, bounded [`SourceRequest`] rather than replaying caller crawl
//! config wholesale. In particular, caller headers are never copied into the
//! web adapter options.

use axon_api::source::{
    AuthSnapshot, ExecutionMode, JobDescriptor, JobPriority, SourceIntent, SourceLimits,
    SourceRefreshPolicy, SourceRequest, SourceScope,
};
use axon_core::config::Config;

use crate::context::ServiceContext;
use crate::source::dispatch::web_options::web_crawl_options;
use crate::source::enqueue::enqueue_source;

pub async fn enqueue_web_source_auto_index(
    cfg: &Config,
    service_context: &ServiceContext,
    url: &str,
    scope: SourceScope,
    max_pages: u64,
    max_depth: u32,
    embed: bool,
    reason: &str,
) -> anyhow::Result<JobDescriptor> {
    let store = service_context
        .job_store()
        .ok_or_else(|| anyhow::anyhow!("unified job store is not available for this runtime"))?;

    let mut request = SourceRequest::new(url.to_string());
    request.intent = SourceIntent::Acquire;
    request.refresh = SourceRefreshPolicy::IfStale;
    request.scope = Some(scope);
    request.embed = embed;
    request.collection = Some(cfg.collection.clone());
    request.execution.mode = ExecutionMode::Background;
    request.execution.detached = true;
    request.execution.priority = JobPriority::Normal;
    request.limits = SourceLimits {
        max_pages: Some(max_pages),
        max_depth: Some(max_depth),
        ..SourceLimits::default()
    };
    request.options.values = web_crawl_options(cfg, Some(max_pages), Some(max_depth));
    request
        .options
        .values
        .insert("discover_sitemaps".to_string(), serde_json::json!(false));
    request
        .options
        .values
        .insert("max_sitemaps".to_string(), serde_json::json!(0u64));
    request
        .options
        .values
        .insert("url_whitelist".to_string(), serde_json::json!([]));
    request
        .options
        .values
        .insert("url_blacklist".to_string(), serde_json::json!([]));
    request
        .options
        .values
        .insert("auto_dispatch_skip".to_string(), serde_json::json!([]));
    request
        .metadata
        .insert("auto_index_reason".to_string(), serde_json::json!(reason));
    request
        .metadata
        .insert("headers_policy".to_string(), serde_json::json!("stripped"));

    let result = enqueue_source(
        request,
        store.as_ref(),
        Some(AuthSnapshot::trusted_system("search-auto-index")),
    )
    .await?;
    let descriptor = result
        .job
        .clone()
        .ok_or_else(|| anyhow::anyhow!(source_enqueue_error(url, &result)))?;
    service_context.notify_unified();
    Ok(descriptor)
}

fn source_enqueue_error(url: &str, result: &axon_api::source::SourceResult) -> String {
    result
        .errors
        .first()
        .map(|error| {
            format!(
                "failed to enqueue source auto-index {url}: {}",
                error.message
            )
        })
        .or_else(|| {
            result.warnings.first().map(|warning| {
                format!(
                    "failed to enqueue source auto-index {url}: {}",
                    warning.message
                )
            })
        })
        .unwrap_or_else(|| format!("failed to enqueue source auto-index {url}: no job descriptor"))
}
