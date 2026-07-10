//! MCP `source` action handler — the unified indexing entrypoint.
//!
//! `source` replaces the removed `embed` / `ingest` / `scrape` / `crawl` /
//! `code_search` / `vertical_scrape` MCP actions. It maps the MCP-facing
//! [`crate::schema::SourceRequest`] onto [`axon_api::source::SourceRequest`] and
//! calls [`axon_services::index_source`], which classifies the input (local
//! path, git URL, feed URL, youtube/reddit target, web URL, session selector,
//! or registry target), acquires it, embeds it, and publishes it through the
//! unified pipeline. The transport-neutral [`axon_api::source::SourceResult`] is
//! returned verbatim as the MCP result payload.

use super::AxonMcpServer;
use super::common::{
    CURRENT_CALLER_AUTH_SNAPSHOT, InlineHint, internal_error, invalid_params,
    logged_internal_error, respond_with_mode, validate_mcp_collection,
};
use crate::schema::{AxonToolResponse, SourceRequest};
use axon_api::source::SourceRequest as ApiSourceRequest;
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(super) async fn handle_source(
        &self,
        req: SourceRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let source = req
            .source
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid_params("source (input) is required for the source action"))?
            .to_string();
        let response_mode = req.response_mode;
        let detached = req.detached.unwrap_or(false);

        let collection = req
            .collection
            .as_deref()
            .map(validate_mcp_collection)
            .transpose()?;

        let mut api_request = ApiSourceRequest::new(source.clone());
        api_request.scope = req.scope;
        api_request.collection = collection;

        // Source indexing needs the data plane (qdrant + tei) and in-process
        // workers — use the fully-provisioned service context, like the other
        // job-running handlers.
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("source.context", e.as_ref()))?;

        // Real caller-derived AuthSnapshot, resolved once in `call_tool`'s
        // scope gate and threaded through via task-local (see
        // `common.rs::CURRENT_CALLER_AUTH_SNAPSHOT`). `None` only in
        // LoopbackDev mode, where there is no per-caller identity to
        // snapshot — the enqueue/inline paths both fall back to
        // `trusted_system` in that case.
        let caller_auth_snapshot = CURRENT_CALLER_AUTH_SNAPSHOT
            .try_with(Clone::clone)
            .unwrap_or_default();

        if detached {
            let Some(job_store) = service_context.job_store() else {
                return Err(internal_error(
                    "source detached=true requires a running job store; retry with detached=false",
                ));
            };
            let result = axon_services::source::enqueue::enqueue_source(
                api_request,
                job_store.as_ref(),
                caller_auth_snapshot,
            )
            .await
            .map_err(|e| logged_internal_error("source.enqueue", e.as_ref()))?;
            let payload = serde_json::to_value(&result)
                .map_err(|e| internal_error(format!("serialize source result: {e}")))?;
            return respond_with_mode(
                "source",
                "source",
                response_mode,
                &format!("source-{}", super::common::slugify(&source, 56)),
                payload,
                InlineHint::Default,
            )
            .await;
        }

        // `index_source` threads a non-`Send` error chain (`Box<dyn Error>`
        // through the crawl_sync ledger), so its future is not `Send` and cannot
        // be awaited directly inside the rmcp `#[tool]` wrapper's `Send` future.
        // Isolate it on a blocking thread with its own current-thread runtime —
        // the same pattern the `evaluate` handler uses. `SourceResult` and the
        // error string both cross the boundary as `Send` values.
        let source_for_task = source.clone();
        let result = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("build source runtime: {e}"))?;
            runtime
                .block_on(axon_services::source::index_source_with_auth(
                    api_request,
                    service_context.as_ref(),
                    caller_auth_snapshot,
                ))
                .map_err(|e| format!("source '{source_for_task}' failed: {e:#}"))
        })
        .await
        .map_err(|e| {
            tracing::error!("join source task: {e}");
            internal_error(format!("source '{source}' failed"))
        })?
        .map_err(|message| {
            tracing::error!("{message}");
            internal_error(message)
        })?;

        let payload = serde_json::to_value(&result)
            .map_err(|e| internal_error(format!("serialize source result: {e}")))?;

        respond_with_mode(
            "source",
            "source",
            response_mode,
            &format!("source-{}", super::common::slugify(&source, 56)),
            payload,
            InlineHint::Default,
        )
        .await
    }
}

#[cfg(test)]
#[path = "handlers_source_tests.rs"]
mod tests;
