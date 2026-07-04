//! MCP handler for the `query` action.

use crate::schema::{AxonToolResponse, QueryRequest};
use crate::server::AxonMcpServer;
use crate::server::common::{
    InlineHint, internal_error, invalid_params, logged_internal_error, respond_with_mode, slugify,
    to_pagination, validate_mcp_collection,
};
use axon_core::config::ConfigOverrides;
use axon_services::query as query_svc;
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(in crate::server) async fn handle_query(
        &self,
        req: QueryRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let query = req
            .query
            .ok_or_else(|| invalid_params("query is required for query"))?;
        let response_mode = req.response_mode;
        let pagination = to_pagination(req.limit, req.offset, self.cfg.search_limit);

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

        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| internal_error(format!("service context: {e}")))?;
        let result = query_svc::query(&ctx, &cfg, &query, pagination)
            .await
            .map_err(|e| logged_internal_error(&format!("query '{query}'"), e.as_ref()))?;

        respond_with_mode(
            "query",
            "query",
            response_mode,
            &format!("query-{}", slugify(&query, 56)),
            serde_json::json!({
                "query": query,
                "limit": pagination.limit,
                "offset": pagination.offset,
                "results": serde_json::to_value(&result.results).map_err(|e| internal_error(format!("serialize query results: {e}")))?,
            }),
            InlineHint::Default,
        )
        .await
    }
}
