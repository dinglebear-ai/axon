use super::AxonMcpServer;
use super::artifacts::{InlineHint, artifact_root, client_context_name, respond_with_mode};
use super::common::{
    CURRENT_PRUNE_AUTHZ, MCP_TOOL_SCHEMA_URI, invalid_params, logged_internal_error,
};
use super::system_requests::{
    CollectionsMcpRequest, CollectionsSubaction, ResetMcpRequest, ResetSubaction,
};
use crate::schema::{AxonToolResponse, DoctorRequest, HelpRequest, PruneMcpRequest, StatusRequest};
use axon_api::source::prune::{PruneRequest as ApiPruneRequest, PruneSelector};
use axon_api::source::{SourceGenerationId, SourceId};
use axon_services::prune::{self, PruneAuthz};
use axon_services::service_traits::{CollectionService, CollectionServiceImpl};
use axon_services::system;
use rmcp::ErrorData;
use serde_json::Value;

const PRUNE_COLLECTION_PREFIX: &str = "collection:";

#[path = "handlers_system/screenshot.rs"]
mod screenshot;
#[path = "handlers_system/uploads.rs"]
mod uploads;

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

        let (plan, result) = prune::prune(&ctx, &api_request, &authz)
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

    pub(super) async fn handle_reset(
        &self,
        req: ResetMcpRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        if req.include_config.unwrap_or(false) {
            return Err(invalid_params(
                "reset does not support deleting configuration",
            ));
        }
        let subaction = req.subaction.unwrap_or_default();
        let mut cfg = (*self.cfg).clone();
        cfg.reset_stores = req.stores.unwrap_or_default();
        if let Some(collection) = req.collection {
            cfg.collection = super::common::validate_mcp_collection(&collection)?;
        }
        if req.include_artifacts == Some(false) {
            if cfg.reset_stores.is_empty() {
                cfg.reset_stores = axon_api::reset::RESET_ALL_STORES
                    .iter()
                    .filter(|store| **store != axon_api::reset::RESET_STORE_ARTIFACTS)
                    .map(|store| (*store).to_string())
                    .collect();
            }
            cfg.reset_stores.retain(|store| store != "artifacts");
        }
        let result = match subaction {
            ResetSubaction::Plan => {
                cfg.reset_dry_run = true;
                axon_services::reset::reset(&cfg).await
            }
            ResetSubaction::Exec => {
                if !req.confirm.unwrap_or(false) {
                    return Err(invalid_params("reset exec requires confirm=true"));
                }
                let plan_id = req
                    .plan_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| invalid_params("reset exec requires plan_id"))?;
                cfg.reset_dry_run = false;
                cfg.yes = true;
                cfg.reset_plan_id = Some(plan_id.to_string());
                let authz = super::common::CURRENT_RESET_AUTHZ
                    .try_with(Clone::clone)
                    .unwrap_or_default();
                axon_services::reset::reset_with_authz(&cfg, &authz).await
            }
        }
        .map_err(|e| invalid_params(e.to_string()))?;
        let label = match subaction {
            ResetSubaction::Plan => "plan",
            ResetSubaction::Exec => "exec",
        };
        respond_with_mode(
            "reset",
            label,
            req.response_mode,
            "reset",
            serde_json::to_value(result).unwrap_or(Value::Null),
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_collections(
        &self,
        req: CollectionsMcpRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let subaction = req.subaction.unwrap_or_default();
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("collections.context", e.as_ref()))?;
        let service = CollectionServiceImpl::new(ctx);
        let mut collections = service
            .list()
            .await
            .map_err(|e| logged_internal_error("collections.list", e.as_ref()))?;
        if let Some(prefix) = req.prefix.as_deref() {
            collections.retain(|item| item.collection.starts_with(prefix));
        }
        let payload = match subaction {
            CollectionsSubaction::List => {
                let offset = req
                    .cursor
                    .as_deref()
                    .unwrap_or("0")
                    .parse::<usize>()
                    .map_err(|_| invalid_params("collections cursor must be a numeric offset"))?;
                let limit = req.limit.unwrap_or(100).clamp(1, 500) as usize;
                let page = collections
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .collect::<Vec<_>>();
                serde_json::json!({"collections": page, "limit": limit, "next_cursor": if page.len() == limit { Some((offset + limit).to_string()) } else { None }})
            }
            CollectionsSubaction::Get => {
                let name = req
                    .collection
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| invalid_params("collections get requires collection"))?;
                let collection = collections
                    .into_iter()
                    .find(|item| item.collection == name)
                    .ok_or_else(|| invalid_params(format!("collection '{name}' not found")))?;
                serde_json::to_value(collection).unwrap_or(Value::Null)
            }
        };
        let label = match subaction {
            CollectionsSubaction::List => "list",
            CollectionsSubaction::Get => "get",
        };
        respond_with_mode(
            "collections",
            label,
            req.response_mode,
            "collections",
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
            "extract": ["start"],
            "jobs": ["list", "get", "status", "events", "stream", "cancel", "retry", "recover", "cleanup", "clear"],
            "memory": ["remember", "list", "search", "show", "link", "supersede", "context", "reinforce", "contradict", "pin", "archive", "forget", "review", "compact", "import", "export"],
            "query": ["query"],
            "retrieve": ["retrieve"],
            "search": ["search"],
            "map": ["map"],
            "prune": ["plan", "exec"],
            "collections": ["list", "get"],
            "uploads": ["list", "create", "get", "put_content", "complete", "abort"],
            "reset": ["plan", "exec"],
            "doctor": ["doctor"],
            "resolve": ["resolve"],
            "capabilities": ["capabilities"],
            "providers": ["list", "get"],
            "diff": ["diff"],
            "brand": ["brand"],
            "watch": ["create", "list", "get", "status", "exec", "history", "update", "pause", "resume", "delete"],
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
