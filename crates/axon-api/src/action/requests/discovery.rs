//! Discovery-tier action request types — resolve / capabilities / providers
//! (issue #298 WS-G). Extracted from `requests.rs` for the monolith line cap.

use serde::{Deserialize, Serialize};

use super::ResponseMode;

/// `action=resolve` — resolve source identity/adapter/route without acquiring
/// content. Backed by `axon_services::source::routing::resolve_source_route`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolveRequest {
    pub source: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

/// `action=capabilities` — machine-readable runtime capability document
/// derived from the live `MCP_ACTION_SPECS` registry and provider doctor
/// data. See `docs/pipeline-unification/surfaces/tool-contract.md`
/// §Capabilities Action.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CapabilitiesRequest {
    pub response_mode: Option<ResponseMode>,
}

/// `action=chat` — direct completion through the configured chat-purpose LLM.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatRequest {
    pub message: Option<String>,
    pub session_id: Option<String>,
    pub response_mode: Option<ResponseMode>,
}

/// `action=providers` — provider capability/health discovery. `list`
/// (default) and `get` (requires `provider_id`) mirror the REST
/// `/v1/providers` resource-tier routes (`crates/axon-web/src/server/
/// handlers/providers.rs`), both backed by `axon_services::system::doctor`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProvidersRequest {
    pub subaction: Option<String>,
    pub provider_id: Option<String>,
    pub response_mode: Option<ResponseMode>,
}
