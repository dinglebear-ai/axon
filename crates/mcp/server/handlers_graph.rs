use super::AxonMcpServer;
use super::common::{
    InlineHint, invalid_params, logged_internal_error, respond_with_mode, slugify,
};
use crate::crates::mcp::schema::{AxonToolResponse, GraphRequest, GraphSubaction};
use crate::crates::services::graph as graph_svc;
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(super) async fn handle_graph(
        &self,
        req: GraphRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("graph.context", e.as_ref()))?;
        if !service_context.capabilities.graph.supported {
            return Err(invalid_params(
                service_context
                    .capabilities
                    .graph
                    .reason
                    .unwrap_or("graph not supported in this mode"),
            ));
        }
        let response_mode = req.response_mode;
        match req.subaction {
            GraphSubaction::Build => {
                let url = req.url.as_deref();
                let domain = req.domain.as_deref();
                let all = req.all.unwrap_or(false);
                if url.is_none() && domain.is_none() && !all {
                    return Err(invalid_params(
                        "graph build requires one of: url, domain, or all=true",
                    ));
                }
                let result = graph_svc::graph_build(self.cfg.as_ref(), url, domain, all)
                    .await
                    .map_err(|e| logged_internal_error("graph.build", e.as_ref()))?;
                let stem = url
                    .map(|value| format!("graph-build-{}", slugify(value, 56)))
                    .or_else(|| domain.map(|value| format!("graph-build-{}", slugify(value, 56))))
                    .unwrap_or_else(|| "graph-build-all".to_string());
                respond_with_mode(
                    "graph",
                    "build",
                    response_mode,
                    &stem,
                    result.payload,
                    InlineHint::Default,
                )
                .await
            }
            GraphSubaction::Status => {
                let result = graph_svc::graph_status(self.cfg.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("graph.status", e.as_ref()))?;
                respond_with_mode(
                    "graph",
                    "status",
                    response_mode,
                    "graph-status",
                    result.payload,
                    InlineHint::Default,
                )
                .await
            }
            GraphSubaction::Explore => {
                let entity = req
                    .entity
                    .ok_or_else(|| invalid_params("graph explore requires entity"))?;
                let result = graph_svc::graph_explore(self.cfg.as_ref(), &entity)
                    .await
                    .map_err(|e| {
                        logged_internal_error(&format!("graph.explore '{entity}'"), e.as_ref())
                    })?;
                respond_with_mode(
                    "graph",
                    "explore",
                    response_mode,
                    &format!("graph-explore-{}", slugify(&entity, 56)),
                    result.payload,
                    InlineHint::Default,
                )
                .await
            }
            GraphSubaction::Stats => {
                let result = graph_svc::graph_stats(self.cfg.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("graph.stats", e.as_ref()))?;
                respond_with_mode(
                    "graph",
                    "stats",
                    response_mode,
                    "graph-stats",
                    result.payload,
                    InlineHint::Default,
                )
                .await
            }
        }
    }
}
