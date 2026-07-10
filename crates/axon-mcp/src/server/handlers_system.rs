use super::AxonMcpServer;
use super::artifacts::{InlineHint, artifact_root, client_context_name, respond_with_mode};
use super::common::{
    CURRENT_PRUNE_AUTHZ, MCP_TOOL_SCHEMA_URI, invalid_params, logged_internal_error,
};
use crate::schema::{AxonToolResponse, DoctorRequest, HelpRequest, PruneMcpRequest, StatusRequest};
use axon_api::source::prune::{PruneRequest as ApiPruneRequest, PruneSelector};
use axon_api::source::{SourceGenerationId, SourceId};
use axon_services::prune::{PruneAuthz, prune};
use axon_services::system;
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

    pub(super) async fn handle_prune(
        &self,
        req: PruneMcpRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let subaction = req.subaction.as_deref().unwrap_or("plan");

        // `dedupe`/`purge` are the target-state replacements for the removed
        // legacy `dedupe`/`purge` MCP actions (U2-24/C6-18/C6-19) — they call
        // the same `axon-services` facades the old actions did, just routed
        // through `prune`'s subaction dispatch and its `axon:admin` gate.
        // They bypass `PruneSelector`/`ApiPruneRequest` entirely since neither
        // maps onto the source/generation/collection prune-selector grammar.
        if subaction == "dedupe" || subaction == "purge" {
            return self.handle_prune_dedupe_or_purge(subaction, &req).await;
        }

        let selector = prune_selector_from_request(&req)?;

        // `prune` executes against the shared, cached `ServiceContext` (see
        // `base_service_context`), which is built once from the server's
        // startup config and cannot be overridden per-call the way the REST
        // `purge` handler (`axon-web`) overrides a throwaway `Config` clone.
        // A per-request `collection` override is therefore not honored here —
        // reject rather than silently ignore it.
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

    /// `prune subaction=dedupe|purge` — thin wrappers over the same
    /// `axon-services::system::{dedupe,purge}` facades the REST
    /// `/v1/prune/dedupe` and `/v1/prune/purge` routes call.
    ///
    /// `purge` here only supports an exact-target delete (`prefix=false`,
    /// `dry_run=!confirm`) — `PruneMcpRequest` has no `prefix`/`dry_run`
    /// fields today (unlike REST's `PurgeRequest`), so prefix-scoped purge
    /// over MCP is a follow-up pending an `axon-api` schema addition.
    async fn handle_prune_dedupe_or_purge(
        &self,
        subaction: &str,
        req: &PruneMcpRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let mut cfg = (*self.cfg).clone();
        if let Some(collection) = req.collection.as_deref() {
            let collection = collection.trim();
            if collection.is_empty() {
                return Err(invalid_params("collection must be non-empty when provided"));
            }
            cfg.collection = collection.to_string();
        }

        let payload = if subaction == "dedupe" {
            let result = system::dedupe(&cfg, None)
                .await
                .map_err(|e| logged_internal_error("prune.dedupe", e.as_ref()))?;
            serde_json::json!({ "subaction": "dedupe", "result": result })
        } else {
            let target = req
                .target
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .ok_or_else(|| invalid_params("purge requires a target"))?;
            let dry_run = !req.confirm.unwrap_or(false);
            let result = system::purge(&cfg, target, false, dry_run)
                .await
                .map_err(|e| logged_internal_error("prune.purge", e.as_ref()))?;
            serde_json::json!({ "subaction": "purge", "result": result })
        };

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

    // `handle_domains`/`handle_sources`/`handle_stats` were removed along
    // with the `domains`/`sources`/`stats` MCP actions (issue #298 WS-G —
    // see the rejection arm in `server.rs`). The underlying
    // `system::domains`/`system::sources`/`system::stats` service functions
    // are untouched and remain reachable through the CLI (`axon domains`,
    // `axon sources`, `axon stats`) and REST.
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
            "prune": ["plan", "exec", "dedupe", "purge"],
            "doctor": ["doctor"],
            "resolve": ["resolve"],
            "capabilities": ["capabilities"],
            "providers": ["list", "get"],
            "diff": ["diff"],
            "brand": ["brand"],
            "watch": ["list", "get", "update", "pause", "resume", "delete"],
            "graph": ["kinds", "resolve", "query", "node", "edge", "source"]
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
