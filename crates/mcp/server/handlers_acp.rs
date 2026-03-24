use super::AxonMcpServer;
use super::common::internal_error;
use crate::crates::mcp::schema::{AcpRequest, AcpSubaction, AxonToolResponse};
use crate::crates::services::acp::session_cache::SESSION_CACHE;
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(super) async fn handle_acp(&self, req: AcpRequest) -> Result<AxonToolResponse, ErrorData> {
        match req.subaction {
            AcpSubaction::ListSessions => self.handle_acp_list_sessions().await,
            AcpSubaction::ForkSession => {
                let session_id = req.session_id.ok_or_else(|| {
                    super::common::invalid_params("session_id is required for fork_session")
                })?;
                self.handle_acp_fork_session(session_id).await
            }
            AcpSubaction::ResumeSession => {
                let session_id = req.session_id.ok_or_else(|| {
                    super::common::invalid_params("session_id is required for resume_session")
                })?;
                self.handle_acp_resume_session(session_id).await
            }
            AcpSubaction::SetModel => {
                let session_id = req.session_id.ok_or_else(|| {
                    super::common::invalid_params("session_id is required for set_model")
                })?;
                let model_id = req.model_id.ok_or_else(|| {
                    super::common::invalid_params("model_id is required for set_model")
                })?;
                self.handle_acp_set_model(session_id, model_id).await
            }
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

    /// Fork an existing ACP session into a new session with the same conversation history.
    ///
    /// TODO: Full implementation requires a new `AdapterMessage::ForkSession` variant and
    /// a corresponding response channel in `AcpConnectionHandle`, so that the request can
    /// be dispatched through the background thread that owns `ClientSideConnection`.
    /// For now this stub validates the session exists and returns a not-implemented response.
    async fn handle_acp_fork_session(
        &self,
        session_id: String,
    ) -> Result<AxonToolResponse, ErrorData> {
        let exists = SESSION_CACHE.get_by_session_id(&session_id).is_some();
        serde_json::to_value(serde_json::json!({
            "session_id": session_id,
            "session_found": exists,
            "status": "not_implemented",
            "message": "fork_session stub: full dispatch via AdapterMessage not yet wired",
        }))
        .map(|data| AxonToolResponse::ok("acp", "fork_session", data))
        .map_err(|e| internal_error(format!("serialize acp/fork_session response: {e}")))
    }

    /// Resume an existing ACP session without replaying message history.
    ///
    /// TODO: Full implementation requires a new `AdapterMessage::ResumeSession` variant and
    /// a corresponding response channel in `AcpConnectionHandle`, so that the request can
    /// be dispatched through the background thread that owns `ClientSideConnection`.
    /// For now this stub validates the session exists and returns a not-implemented response.
    async fn handle_acp_resume_session(
        &self,
        session_id: String,
    ) -> Result<AxonToolResponse, ErrorData> {
        let exists = SESSION_CACHE.get_by_session_id(&session_id).is_some();
        serde_json::to_value(serde_json::json!({
            "session_id": session_id,
            "session_found": exists,
            "status": "not_implemented",
            "message": "resume_session stub: full dispatch via AdapterMessage not yet wired",
        }))
        .map(|data| AxonToolResponse::ok("acp", "resume_session", data))
        .map_err(|e| internal_error(format!("serialize acp/resume_session response: {e}")))
    }

    /// Set the active model for an ACP session via `session/set_model`.
    ///
    /// TODO: Full implementation requires a new `AdapterMessage::SetSessionModel` variant and
    /// a corresponding response channel in `AcpConnectionHandle`, so that the request can
    /// be dispatched through the background thread that owns `ClientSideConnection`.
    /// For now this stub validates the session exists and returns a not-implemented response.
    async fn handle_acp_set_model(
        &self,
        session_id: String,
        model_id: String,
    ) -> Result<AxonToolResponse, ErrorData> {
        let exists = SESSION_CACHE.get_by_session_id(&session_id).is_some();
        serde_json::to_value(serde_json::json!({
            "session_id": session_id,
            "model_id": model_id,
            "session_found": exists,
            "status": "not_implemented",
            "message": "set_model stub: full dispatch via AdapterMessage not yet wired",
        }))
        .map(|data| AxonToolResponse::ok("acp", "set_model", data))
        .map_err(|e| internal_error(format!("serialize acp/set_model response: {e}")))
    }
}
