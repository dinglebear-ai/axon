//! MCP handler for the `query` action.

use crate::core::config::ConfigOverrides;
use crate::mcp::schema::{AxonToolResponse, QueryRequest};
use crate::mcp::server::AxonMcpServer;
use crate::mcp::server::common::{
    InlineHint, internal_error, invalid_params, logged_internal_error, parse_offset,
    respond_with_mode, slugify, to_pagination, validate_mcp_collection,
};
use crate::services::query as query_svc;
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(in crate::mcp::server) async fn handle_query(
        &self,
        req: QueryRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let query = req
            .query
            .ok_or_else(|| invalid_params("query is required for query"))?;
        let limit = req.limit.unwrap_or(self.cfg.search_limit).clamp(1, 500);
        let offset = parse_offset(req.offset);
        let response_mode = req.response_mode;
        let pagination = to_pagination(Some(limit), Some(offset), self.cfg.search_limit);

        let collection = req
            .collection
            .as_deref()
            .map(validate_mcp_collection)
            .transpose()?;
        let cfg = self.cfg.apply_overrides(&ConfigOverrides {
            collection,
            since: req.since,
            before: req.before,
            hybrid_search_enabled: req.hybrid_search,
            ..ConfigOverrides::default()
        });

        let result = query_svc::query(&cfg, &query, pagination)
            .await
            .map_err(|e| logged_internal_error(&format!("query '{query}'"), e.as_ref()))?;

        respond_with_mode(
            "query",
            "query",
            response_mode,
            &format!("query-{}", slugify(&query, 56)),
            serde_json::json!({
                "query": query,
                "limit": limit,
                "offset": offset,
                "results": serde_json::to_value(&result.results).map_err(|e| internal_error(format!("serialize query results: {e}")))?,
            }),
            InlineHint::Default,
        )
        .await
    }
}
