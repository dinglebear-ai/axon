use super::AxonMcpServer;
use super::artifacts::{InlineHint, artifact_root, client_context_name, respond_with_mode};
use super::common::{
    CURRENT_PRUNE_AUTHZ, MCP_TOOL_SCHEMA_URI, invalid_params, logged_internal_error, to_pagination,
};
use crate::schema::{
    AxonToolResponse, DoctorRequest, DomainsRequest, HelpRequest, PruneMcpRequest, PurgeRequest,
    SourcesRequest, StatsRequest, StatusRequest,
};
use axon_api::source::prune::{PruneRequest as ApiPruneRequest, PruneSelector};
use axon_api::source::{SourceGenerationId, SourceId};
use axon_services::prune::{PruneAuthz, prune};
use axon_services::system;
use axon_services::transport;
use rmcp::ErrorData;
use serde_json::Value;

const PRUNE_COLLECTION_PREFIX: &str = "collection:";

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

    pub(super) async fn handle_purge(
        &self,
        req: PurgeRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let target = req
            .target
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid_params("purge requires a target URL"))?
            .to_string();
        // Agent safety: a bare `purge` previews; deletion requires dry_run=false.
        let dry_run = req.dry_run.unwrap_or(true);
        let mut cfg = (*self.cfg).clone();
        if let Some(collection) = req.collection.as_deref() {
            axon_core::config::validate_collection_name(collection)
                .map_err(|e| invalid_params(format!("collection: {e}")))?;
            cfg.collection = collection.to_string();
        }
        let result = system::purge(&cfg, &target, req.prefix, dry_run)
            .await
            .map_err(|e| logged_internal_error("purge", e.as_ref()))?;
        let payload =
            serde_json::to_value(result).map_err(|e| logged_internal_error("purge", &e))?;
        respond_with_mode(
            "purge",
            "purge",
            req.response_mode,
            "purge",
            payload,
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_prune(
        &self,
        req: PruneMcpRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let subaction = req.subaction.as_deref().unwrap_or("plan");
        let selector = prune_selector_from_request(&req)?;

        // `prune` executes against the shared, cached `ServiceContext` (see
        // `base_service_context`), which is built once from the server's
        // startup config and cannot be overridden per-call the way
        // `handle_purge` overrides a throwaway `Config` clone. A per-request
        // `collection` override is therefore not honored here — reject
        // rather than silently ignore it.
        if req.collection.is_some() {
            return Err(invalid_params(
                "prune does not support a per-request collection override over MCP; the server's configured collection is always used",
            ));
        }

        let api_request = match subaction {
            "plan" => ApiPruneRequest::dry_run(selector, "mcp prune plan"),
            "exec" => {
                if !req.confirm.unwrap_or(false) {
                    return Err(invalid_params(
                        "prune exec requires confirm=true to run destructively",
                    ));
                }
                ApiPruneRequest::execute(selector, "mcp prune exec")
            }
            other => {
                return Err(invalid_params(format!(
                    "unknown prune subaction '{other}' (expected plan|exec)"
                )));
            }
        };

        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("prune.context", e.as_ref()))?;

        // Real caller-derived authz — resolved once in `call_tool`'s scope
        // gate and threaded through via task-local (see module docs on
        // `CURRENT_PRUNE_AUTHZ`). Never hardcoded here.
        let authz: PruneAuthz = CURRENT_PRUNE_AUTHZ
            .try_with(Clone::clone)
            .unwrap_or_default();

        let (plan, result) = prune(&ctx, &api_request, &authz)
            .await
            .map_err(|e| invalid_params(e.to_string()))?;

        let payload = serde_json::json!({
            "subaction": subaction,
            "plan": plan,
            "result": result,
        });
        respond_with_mode(
            "prune",
            subaction,
            req.response_mode,
            "prune",
            payload,
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
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("doctor", e.as_ref()))?;
        let result = system::doctor(&ctx)
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

/// Build a [`PruneSelector`] from an MCP [`PruneMcpRequest`]. Mirrors
/// `crates/axon-cli/src/commands/prune.rs::build_selector`'s
/// `collection:<name>` / bare-source-id grammar.
fn prune_selector_from_request(req: &PruneMcpRequest) -> Result<PruneSelector, ErrorData> {
    let target = req
        .target
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            invalid_params("prune requires a target (source id, or collection:<name>)")
        })?;

    if let Some(collection) = target.strip_prefix(PRUNE_COLLECTION_PREFIX) {
        let collection = collection.trim();
        if collection.is_empty() {
            return Err(invalid_params(
                "collection: target requires a non-empty collection name",
            ));
        }
        if req.generation.is_some() {
            return Err(invalid_params(
                "generation is not valid with a collection: target",
            ));
        }
        return Ok(PruneSelector::Collection {
            collection: collection.to_string(),
        });
    }

    let source_id = SourceId::new(target);
    Ok(
        match req
            .generation
            .as_deref()
            .map(str::trim)
            .filter(|g| !g.is_empty())
        {
            Some(generation) => PruneSelector::Generation {
                source_id,
                generation: SourceGenerationId::new(generation),
            },
            None => PruneSelector::Source { source_id },
        },
    )
}

fn help_payload() -> Value {
    serde_json::json!({
        "tool": "axon",
        "actions": {
            "status": [],
            "help": [],
            "source": ["source"],
            "summarize": ["summarize"],
            "research": ["research"],
            "ask": ["ask"],
            "evaluate": ["evaluate"],
            "suggest": ["suggest"],
            "screenshot": ["screenshot"],
            "endpoints": ["endpoints"],
            "extract": ["start", "status", "cancel", "list", "cleanup", "clear", "recover"],
            "jobs": ["list", "get", "status", "events", "stream", "artifacts", "cancel", "retry", "recover", "cleanup", "clear"],
            "memory": ["remember", "list", "search", "show", "link", "supersede", "context", "reinforce", "contradict", "pin", "archive", "forget", "review", "compact", "import", "export"],
            "query": ["query"],
            "retrieve": ["retrieve"],
            "search": ["search"],
            "map": ["map"],
            "purge": ["purge"],
            "prune": ["plan", "exec"],
            "doctor": ["doctor"],
            "domains": ["domains"],
            "sources": ["sources"],
            "stats": ["stats"],
            "diff": ["diff"],
            "brand": ["brand"],
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
