//! `resolve`, `capabilities`, and `providers` MCP actions (issue #298 WS-G).
//!
//! All three are read-only discovery surfaces backed by real data already
//! available in-process — no new external calls, no stubbed responses:
//! - `resolve` calls `axon_services::source::routing::resolve_source_route`,
//!   the same resolver/router the `source` action's acquisition path uses.
//! - `providers` reshapes `axon_services::system::doctor`'s per-service
//!   payload into a stable provider list/detail shape, mirroring the REST
//!   resource-tier routes in `crates/axon-web/src/server/handlers/
//!   providers.rs` (same backing call, no separate provider-capability
//!   service exists yet — see WS-G followups).
//! - `capabilities` reports the live `MCP_ACTION_SPECS` registry (the same
//!   source of truth `tool_schema.rs` derives the tool's input schema from)
//!   plus the same doctor-derived provider summaries `providers` returns.

use super::AxonMcpServer;
use super::artifacts::{InlineHint, respond_with_mode};
use super::common::{invalid_params, logged_internal_error};
use super::server_authz::MCP_ACTION_SPECS;
use crate::schema::{AxonToolResponse, CapabilitiesRequest, ProvidersRequest, ResolveRequest};
use axon_api::source::SourceRequest as RouteSourceRequest;
use axon_services::system;
use rmcp::ErrorData;
use serde_json::Value;

impl AxonMcpServer {
    pub(super) async fn handle_resolve(
        &self,
        req: ResolveRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let source = req
            .source
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid_params("resolve requires a non-empty `source`"))?;

        let route_request = RouteSourceRequest::new(source);
        let routed = axon_services::source::routing::resolve_source_route(&route_request)
            .map_err(|e| invalid_params(format!("source.resolve.unsupported: {e}")))?;

        let payload = serde_json::json!({
            "source": source,
            "kind": format!("{:?}", routed.kind),
            "route": routed.route,
        });
        respond_with_mode(
            "resolve",
            "resolve",
            req.response_mode,
            "resolve",
            payload,
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_providers(
        &self,
        req: ProvidersRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let subaction = req.subaction.as_deref().unwrap_or("list");
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("providers.context", e.as_ref()))?;
        let doctor = system::doctor(&ctx)
            .await
            .map_err(|e| logged_internal_error("providers.doctor", e.as_ref()))?;
        let providers = provider_summaries(&doctor.payload);

        let payload = match subaction {
            "list" => serde_json::json!({ "providers": providers }),
            "get" => {
                let provider_id = req
                    .provider_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| invalid_params("providers.get requires `provider_id`"))?;
                let found = providers
                    .into_iter()
                    .find(|p| p["id"] == provider_id)
                    .ok_or_else(|| {
                        invalid_params(format!(
                            "provider.unavailable: unknown provider `{provider_id}`"
                        ))
                    })?;
                found
            }
            other => {
                return Err(invalid_params(format!(
                    "unknown providers subaction '{other}' (expected list|get)"
                )));
            }
        };

        respond_with_mode(
            "providers",
            subaction,
            req.response_mode,
            "providers",
            payload,
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_capabilities(
        &self,
        req: CapabilitiesRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("capabilities.context", e.as_ref()))?;
        let doctor = system::doctor(&ctx)
            .await
            .map_err(|e| logged_internal_error("capabilities.doctor", e.as_ref()))?;
        let providers = provider_summaries(&doctor.payload);

        let actions: Vec<Value> = MCP_ACTION_SPECS
            .iter()
            .map(|spec| {
                serde_json::json!({
                    "name": spec.name,
                    "scope": spec.scope.as_label(),
                    "cost": spec.cost,
                    "description": spec.description,
                })
            })
            .collect();

        let payload = serde_json::json!({
            "server": {
                "name": "axon",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "contract_version": axon_api::mcp_schema::MCP_CONTRACT_VERSION,
            "actions": actions,
            "providers": providers,
        });

        respond_with_mode(
            "capabilities",
            "capabilities",
            req.response_mode,
            "capabilities",
            payload,
            InlineHint::Default,
        )
        .await
    }
}

/// Reshape `doctor()`'s `{"services": {"<id>": {"ok": bool, ...}}}` payload
/// into a stable, sorted provider list. Duplicated (not shared) from the REST
/// `providers.rs` handler because that crate is out of this territory's
/// write scope; both project the exact same `doctor()` payload shape.
fn provider_summaries(doctor_payload: &Value) -> Vec<Value> {
    let Some(services_map) = doctor_payload.get("services").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut providers: Vec<Value> = services_map
        .iter()
        .map(|(id, detail)| {
            serde_json::json!({
                "id": id,
                "ok": detail.get("ok").and_then(Value::as_bool).unwrap_or(false),
                "detail": detail,
            })
        })
        .collect();
    providers.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    providers
}

#[cfg(test)]
#[path = "handlers_discovery_tests.rs"]
mod tests;
