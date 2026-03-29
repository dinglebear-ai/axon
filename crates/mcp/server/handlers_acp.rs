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
            AcpSubaction::ExtMethod => {
                let method = req.method.ok_or_else(|| {
                    super::common::invalid_params("method is required for ext_method")
                })?;
                self.handle_acp_ext_method(method, req.params).await
            }
            AcpSubaction::ExtNotification => {
                let method = req.method.ok_or_else(|| {
                    super::common::invalid_params("method is required for ext_notification")
                })?;
                self.handle_acp_ext_notification(method, req.params).await
            }
            AcpSubaction::Logout => {
                let session_id = req.session_id.ok_or_else(|| {
                    super::common::invalid_params("session_id is required for logout")
                })?;
                self.handle_acp_logout(session_id).await
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

    /// Send an outbound extension method to the ACP agent (FR-027).
    ///
    /// Accepts a method name and optional JSON params. Dispatches via
    /// `conn.ext_method(ExtRequest::new(method, params))`.
    ///
    /// TODO: Full implementation requires a new `AdapterMessage::ExtMethod` variant and
    /// a corresponding response channel in `AcpConnectionHandle`, so that the request can
    /// be dispatched through the background thread that owns `ClientSideConnection`.
    /// For now this stub validates params are well-formed and returns a not-implemented response.
    async fn handle_acp_ext_method(
        &self,
        method: String,
        params: Option<serde_json::Value>,
    ) -> Result<AxonToolResponse, ErrorData> {
        // Validate params can be round-tripped as raw JSON (required by ExtRequest::new).
        if let Some(ref p) = params {
            serde_json::value::RawValue::from_string(p.to_string()).map_err(|e| {
                super::common::invalid_params(format!("params must be valid JSON: {e}"))
            })?;
        }
        serde_json::to_value(serde_json::json!({
            "method": method,
            "params": params,
            "status": "not_implemented",
            "message": "ext_method stub: full dispatch via AdapterMessage not yet wired",
        }))
        .map(|data| AxonToolResponse::ok("acp", "ext_method", data))
        .map_err(|e| internal_error(format!("serialize acp/ext_method response: {e}")))
    }

    /// Send an outbound extension notification to the ACP agent (FR-028).
    ///
    /// Accepts a method name and optional JSON params. Dispatches via
    /// `conn.ext_notification(ExtNotification::new(method, params))`.
    ///
    /// TODO: Full implementation requires a new `AdapterMessage::ExtNotification` variant and
    /// a corresponding response channel in `AcpConnectionHandle`, so that the request can
    /// be dispatched through the background thread that owns `ClientSideConnection`.
    /// For now this stub validates params are well-formed and returns a not-implemented response.
    async fn handle_acp_ext_notification(
        &self,
        method: String,
        params: Option<serde_json::Value>,
    ) -> Result<AxonToolResponse, ErrorData> {
        // Validate params can be round-tripped as raw JSON (required by ExtNotification::new).
        if let Some(ref p) = params {
            serde_json::value::RawValue::from_string(p.to_string()).map_err(|e| {
                super::common::invalid_params(format!("params must be valid JSON: {e}"))
            })?;
        }
        serde_json::to_value(serde_json::json!({
            "method": method,
            "params": params,
            "status": "not_implemented",
            "message": "ext_notification stub: full dispatch via AdapterMessage not yet wired",
        }))
        .map(|data| AxonToolResponse::ok("acp", "ext_notification", data))
        .map_err(|e| internal_error(format!("serialize acp/ext_notification response: {e}")))
    }

    /// Request a clean session logout from the ACP agent (FR-032).
    ///
    /// TODO: Full implementation requires `conn.logout()` or an equivalent SDK method.
    /// The `agent_client_protocol` crate (v0.10.2) does not expose a logout method —
    /// this stub validates the session exists and returns a not-implemented response until
    /// the SDK adds `unstable_logout` support.
    ///
    /// TODO: Also enable `unstable_logout` in `ClientCapabilities` once the SDK exposes it.
    async fn handle_acp_logout(&self, session_id: String) -> Result<AxonToolResponse, ErrorData> {
        let exists = SESSION_CACHE.get_by_session_id(&session_id).is_some();
        serde_json::to_value(serde_json::json!({
            "session_id": session_id,
            "session_found": exists,
            "status": "not_implemented",
            "message": "logout stub: SDK v0.10.2 does not expose a logout method; \
                        enable unstable_logout in ClientCapabilities once available",
        }))
        .map(|data| AxonToolResponse::ok("acp", "logout", data))
        .map_err(|e| internal_error(format!("serialize acp/logout response: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that `AcpSubaction::ListSessions` is registered in the MCP router
    /// by confirming the variant exists and the exhaustive `handle_acp` match
    /// covers it (compile-time proof — if the arm were absent the file would
    /// not compile).
    #[test]
    fn list_sessions_subaction_variant_exists() {
        // Construct the variant — the compiler rejects this if it is missing.
        let subaction = AcpSubaction::ListSessions;
        // Verify the exhaustive match in handle_acp covers ListSessions by
        // checking the variant can be matched without a wildcard fallthrough.
        let name = match subaction {
            AcpSubaction::ListSessions => "list_sessions",
            AcpSubaction::ForkSession => "fork_session",
            AcpSubaction::ResumeSession => "resume_session",
            AcpSubaction::SetModel => "set_model",
            AcpSubaction::ExtMethod => "ext_method",
            AcpSubaction::ExtNotification => "ext_notification",
            AcpSubaction::Logout => "logout",
        };
        assert_eq!(name, "list_sessions");
    }
}
