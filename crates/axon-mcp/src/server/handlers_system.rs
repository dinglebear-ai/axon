use super::AxonMcpServer;
use super::artifacts::{InlineHint, artifact_root, client_context_name, respond_with_mode};
use super::common::{MCP_TOOL_SCHEMA_URI, logged_internal_error, to_pagination};
use crate::schema::{
    AxonToolResponse, DoctorRequest, DomainsRequest, HelpRequest, SourcesRequest, StatsRequest,
    StatusRequest,
};
use axon_services::system;
use axon_services::transport;
use rmcp::ErrorData;
use serde_json::Value;

#[path = "handlers_system/screenshot.rs"]
mod screenshot;

#[cfg(test)]
#[path = "handlers_system_tests.rs"]
mod tests;

// --- Public handlers ---

impl AxonMcpServer {
    pub(super) async fn handle_help(
        &self,
        req: HelpRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        respond_with_mode(
            "help",
            "help",
            req.response_mode,
            "help-actions",
            help_payload(),
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_status(
        &self,
        req: StatusRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("status", e.as_ref()))?;
        let result = system::full_status(&ctx)
            .await
            .map_err(|e| logged_internal_error("status", e.as_ref()))?;
        // Status is the primary widget data source; always inline so the MCP
        // App dashboard can render it without needing an HTTP-accessible artifact URL.
        respond_with_mode(
            "status",
            "status",
            response_mode,
            "status",
            result.payload,
            InlineHint::Document,
        )
        .await
    }

    pub(super) async fn handle_doctor(
        &self,
        req: DoctorRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        let result = system::doctor(self.cfg.as_ref())
            .await
            .map_err(|e| logged_internal_error("doctor", e.as_ref()))?;
        respond_with_mode(
            "doctor",
            "doctor",
            response_mode,
            "doctor",
            result.payload,
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_domains(
        &self,
        req: DomainsRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let pagination = to_pagination(req.limit, req.offset, transport::DISCOVERY_PAGE_DEFAULT);
        let response_mode = req.response_mode;
        if let Some(domain) = req.domain.as_deref() {
            let result = system::domain_indexed(self.cfg.as_ref(), domain)
                .await
                .map_err(|e| logged_internal_error("domains", e.as_ref()))?;
            let payload =
                serde_json::to_value(result).map_err(|e| logged_internal_error("domains", &e))?;
            return respond_with_mode(
                "domains",
                "domains",
                response_mode,
                "domains",
                payload,
                InlineHint::Default,
            )
            .await;
        }
        let result = system::domains(self.cfg.as_ref(), pagination)
            .await
            .map_err(|e| logged_internal_error("domains", e.as_ref()))?;
        let payload = serde_json::json!({
            "limit": result.limit,
            "offset": result.offset,
            "domains": result.domains.iter().map(|d| serde_json::json!({
                "domain": d.domain,
                "vectors": d.vectors,
            })).collect::<Vec<_>>(),
        });
        respond_with_mode(
            "domains",
            "domains",
            response_mode,
            "domains",
            payload,
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_sources(
        &self,
        req: SourcesRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        if let Some(domain) = req.domain.as_deref() {
            let pagination = transport::domain_sources_pagination(req.limit, req.offset);
            let result = system::sources_for_domain(
                self.cfg.as_ref(),
                domain,
                pagination,
                req.cursor.as_deref(),
            )
            .await
            .map_err(|e| logged_internal_error("sources", e.as_ref()))?;
            let payload =
                serde_json::to_value(result).map_err(|e| logged_internal_error("sources", &e))?;
            return respond_with_mode(
                "sources",
                "sources",
                response_mode,
                "sources",
                payload,
                InlineHint::Default,
            )
            .await;
        }
        let pagination = to_pagination(req.limit, req.offset, transport::DISCOVERY_PAGE_DEFAULT);
        let result = system::sources(self.cfg.as_ref(), pagination)
            .await
            .map_err(|e| logged_internal_error("sources", e.as_ref()))?;
        let payload =
            serde_json::to_value(result).map_err(|e| logged_internal_error("sources", &e))?;
        respond_with_mode(
            "sources",
            "sources",
            response_mode,
            "sources",
            payload,
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_stats(
        &self,
        req: StatsRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        let result = system::stats(self.cfg.as_ref())
            .await
            .map_err(|e| logged_internal_error("stats", e.as_ref()))?;
        respond_with_mode(
            "stats",
            "stats",
            response_mode,
            "stats",
            result.payload,
            InlineHint::Default,
        )
        .await
    }
}

fn help_payload() -> Value {
    serde_json::json!({
        "tool": "axon",
        "actions": {
            "status": [],
            "help": [],
            "scrape": ["scrape"],
            "summarize": ["summarize"],
            "research": ["research"],
            "ask": ["ask"],
            "evaluate": ["evaluate"],
            "suggest": ["suggest"],
            "screenshot": ["screenshot"],
            "endpoints": ["endpoints"],
            "crawl": ["start", "status", "cancel", "list", "cleanup", "clear", "recover"],
            "extract": ["start", "status", "cancel", "list", "cleanup", "clear", "recover"],
            "embed": ["start", "status", "cancel", "list", "cleanup", "clear", "recover"],
            "ingest": ["start", "status", "cancel", "list", "cleanup", "clear", "recover"],
            "memory": ["remember", "list", "search", "show", "link", "supersede", "context"],
            "query": ["query"],
            "code_search": [],
            "retrieve": ["retrieve"],
            "search": ["search"],
            "map": ["map"],
            "doctor": ["doctor"],
            "domains": ["domains"],
            "sources": ["sources"],
            "stats": ["stats"],
            "diff": ["diff"],
            "brand": ["brand"],
            "vertical_scrape": ["list", "capabilities"],
            "elicit_demo": []
        },
        "resources": [
            MCP_TOOL_SCHEMA_URI
        ],
        "defaults": {
            "response_mode": "path",
            "artifact_dir": artifact_root(),
            "artifact_context": client_context_name()
        }
    })
}
