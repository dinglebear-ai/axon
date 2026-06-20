//! MCP handler for the `code_search` action.

use crate::core::config::ConfigOverrides;
use crate::mcp::schema::{AxonToolResponse, CodeSearchRequest};
use crate::mcp::server::AxonMcpServer;
use crate::mcp::server::common::{
    InlineHint, internal_error, invalid_params, logged_internal_error, parse_offset,
    respond_with_mode, slugify, validate_mcp_collection,
};
use crate::services::query as query_svc;
use crate::services::types::{CodeSearchCaller, CodeSearchOptions};
use rmcp::ErrorData;
use std::path::PathBuf;

impl AxonMcpServer {
    pub(in crate::mcp::server) async fn handle_code_search(
        &self,
        req: CodeSearchRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let query = req
            .query
            .ok_or_else(|| invalid_params("query is required for code_search"))?;
        let cwd = req
            .cwd
            .ok_or_else(|| invalid_params("cwd is required for code_search MCP requests"))?;
        let limit = req.limit.unwrap_or(self.cfg.search_limit).clamp(1, 500);
        let offset = parse_offset(req.offset);
        let response_mode = req.response_mode;
        let collection = req
            .collection
            .as_deref()
            .map(validate_mcp_collection)
            .transpose()?;
        let cfg = self.cfg.apply_overrides(&ConfigOverrides {
            collection,
            ..ConfigOverrides::default()
        });
        let ctx = self
            .service_context_for(cfg)
            .await
            .map_err(|e| logged_internal_error("code_search.context", e.as_ref()))?;

        let result = query_svc::code_search(
            &ctx,
            &query,
            CodeSearchOptions {
                limit,
                offset,
                cwd: Some(PathBuf::from(cwd)),
                path_prefix: req.path_prefix,
                ensure_fresh: !req.no_freshness.unwrap_or(false),
                caller: CodeSearchCaller::Mcp,
            },
        )
        .await
        .map_err(|e| code_search_error(&query, e.as_ref()))?;

        respond_with_mode(
            "code_search",
            "code_search",
            response_mode,
            &format!("code-search-{}", slugify(&query, 56)),
            serde_json::to_value(result)
                .map_err(|e| internal_error(format!("serialize code_search result: {e}")))?,
            InlineHint::Default,
        )
        .await
    }
}

fn code_search_error(
    query: &str,
    error: &(dyn std::error::Error + Send + Sync + 'static),
) -> ErrorData {
    let message = error.to_string();
    if is_code_search_invalid_params(&message) {
        invalid_params(message)
    } else {
        logged_internal_error(&format!("code_search '{query}'"), error)
    }
}

fn is_code_search_invalid_params(message: &str) -> bool {
    message.starts_with("code_search query exceeds ")
        || matches!(
            message,
            "code_search MCP requests must provide cwd"
                | "code_search cwd could not be resolved"
                | "code_search cwd is not inside a git checkout"
                | "code_search cwd is outside AXON_CODE_SEARCH_ALLOWED_ROOTS"
                | "code_search refuses to index filesystem root"
                | "code_search refuses to index HOME directly"
                | "path_prefix must be repository-relative"
                | "path_prefix cannot escape the repository root"
        )
}
