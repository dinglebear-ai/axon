//! Thin MCP adapters for the restored focused projection actions.

use super::AxonMcpServer;
use super::common::{CURRENT_CALLER_AUTH_SNAPSHOT, invalid_params, logged_internal_error};
use crate::schema::AxonToolResponse;
use axon_api::source::*;
use axon_services::projections::{
    SourceAccessPolicy, execute_code_search_projection_batch, execute_source_projection_batch,
    preflight_code_search_batch, preflight_source_batch,
};
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(super) async fn handle_scrape_projection(
        &self,
        request: ScrapeRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        self.handle_source_projection(
            ProjectionOperation::Scrape,
            project_scrape(&request).map_err(projection_input_error)?,
        )
        .await
    }

    pub(super) async fn handle_crawl_projection(
        &self,
        request: CrawlRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        self.handle_source_projection(
            ProjectionOperation::Crawl,
            project_crawl(&request).map_err(projection_input_error)?,
        )
        .await
    }

    pub(super) async fn handle_embed_projection(
        &self,
        request: EmbedRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        self.handle_source_projection(
            ProjectionOperation::Embed,
            project_embed(&request).map_err(projection_input_error)?,
        )
        .await
    }

    pub(super) async fn handle_ingest_projection(
        &self,
        request: IngestRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        self.handle_source_projection(
            ProjectionOperation::Ingest,
            project_ingest(&request).map_err(projection_input_error)?,
        )
        .await
    }

    async fn handle_source_projection(
        &self,
        operation: ProjectionOperation,
        requests: Vec<SourceRequest>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let auth = current_auth();
        let access = SourceAccessPolicy {
            operator_allows_tool_execution: self.cfg.allow_tool_execution,
            allowed_roots: Some(self.cfg.source_local_allowed_roots.clone()),
            ..SourceAccessPolicy::default()
        };
        let prepared = preflight_source_batch(
            operation,
            requests,
            auth.as_ref(),
            &self.cfg.projection_batch,
            &access,
        )
        .map_err(projection_preflight_error)?;
        let ctx = self
            .base_service_context()
            .await
            .map_err(|error| logged_internal_error("projection.context", error.as_ref()))?;
        let result = execute_source_projection_batch(ctx.as_ref(), operation, prepared, auth)
            .await
            .map_err(|error| logged_internal_error("projection.execute", &error))?;
        projection_response(operation, result)
    }

    pub(super) async fn handle_code_search_projection(
        &self,
        request: CodeSearchRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let plans = project_code_search(&request).map_err(projection_input_error)?;
        let prepared = preflight_code_search_batch(plans, &self.cfg.projection_batch)
            .map_err(projection_preflight_error)?;
        let ctx = self
            .base_service_context()
            .await
            .map_err(|error| logged_internal_error("code_search.context", error.as_ref()))?;
        let handle = tokio::runtime::Handle::current();
        let result = tokio::task::spawn_blocking(move || {
            handle.block_on(execute_code_search_projection_batch(
                ctx.as_ref(),
                prepared,
                axon_api::CodeSearchCaller::Mcp,
            ))
        })
        .await
        .map_err(|error| super::common::internal_error(format!("code_search task: {error}")))?
        .map_err(|error| logged_internal_error("code_search.execute", &error))?;
        projection_response(ProjectionOperation::CodeSearch, result)
    }
}

fn current_auth() -> Option<AuthSnapshot> {
    CURRENT_CALLER_AUTH_SNAPSHOT
        .try_with(Clone::clone)
        .unwrap_or_default()
}

fn projection_input_error(error: Box<ApiError>) -> ErrorData {
    invalid_params(error.to_string())
}

fn projection_preflight_error(error: ApiError) -> ErrorData {
    invalid_params(error.to_string())
}

fn projection_response<T: serde::Serialize>(
    operation: ProjectionOperation,
    result: BatchResult<T>,
) -> Result<AxonToolResponse, ErrorData> {
    let action = match operation {
        ProjectionOperation::CodeSearch => "code_search",
        ProjectionOperation::Scrape => "scrape",
        ProjectionOperation::Crawl => "crawl",
        ProjectionOperation::Embed => "embed",
        ProjectionOperation::Ingest => "ingest",
    };
    let data = serde_json::to_value(result)
        .map_err(|error| super::common::internal_error(format!("serialize {action}: {error}")))?;
    Ok(AxonToolResponse::ok(action, action, data))
}

#[cfg(test)]
#[path = "handlers_projections_tests.rs"]
mod tests;
