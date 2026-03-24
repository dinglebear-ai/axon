use super::AxonMcpServer;
use super::common::internal_error;
use crate::crates::mcp::schema::{AcpRequest, AcpSubaction, AxonToolResponse};
use crate::crates::services::acp::session_cache::SESSION_CACHE;
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(super) async fn handle_acp(&self, req: AcpRequest) -> Result<AxonToolResponse, ErrorData> {
        match req.subaction {
            AcpSubaction::ListSessions => self.handle_acp_list_sessions().await,
        }
    }

    async fn handle_acp_list_sessions(&self) -> Result<AxonToolResponse, ErrorData> {
        let agent_keys = SESSION_CACHE.agent_keys();
        let count = agent_keys.len();
        let sessions: Vec<serde_json::Value> = agent_keys
            .into_iter()
            .map(|key| serde_json::json!({ "agent_key": key }))
            .collect();

        serde_json::to_value(serde_json::json!({
            "count": count,
            "sessions": sessions,
        }))
        .map(|data| AxonToolResponse::ok("acp", "list_sessions", data))
        .map_err(|e| internal_error(format!("serialize acp/list_sessions response: {e}")))
    }
}
