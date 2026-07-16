use super::AxonMcpServer;
use super::common::{
    CURRENT_CALLER_AUTH_SNAPSHOT, apply_extract_overrides, invalid_params, logged_internal_error,
    validate_mcp_urls,
};
use crate::schema::{AxonToolResponse, ExtractRequest, ExtractSubaction};
use axon_services::extract as extract_svc;
use rmcp::ErrorData;

impl AxonMcpServer {
    async fn handle_extract_start(
        &self,
        req: ExtractRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let cfg = apply_extract_overrides(self.cfg.as_ref(), &req);
        let urls = req
            .urls
            .ok_or_else(|| invalid_params("urls is required for extract.start"))?;
        if urls.is_empty() {
            return Err(invalid_params("urls cannot be empty"));
        }
        validate_mcp_urls(&urls)?;
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("extract.start.context", e.as_ref()))?;
        // Real caller-derived AuthSnapshot, resolved once in `call_tool`'s
        // scope gate and threaded through via task-local (see
        // `common.rs::CURRENT_CALLER_AUTH_SNAPSHOT`). `None` only in
        // LoopbackDev mode, where there is no per-caller identity to
        // snapshot — `extract_start_with_context` falls back to
        // `trusted_system` in that case, same as before.
        let caller_auth_snapshot = CURRENT_CALLER_AUTH_SNAPSHOT
            .try_with(Clone::clone)
            .unwrap_or_default();
        let outcome = extract_svc::extract_start_with_context(
            &cfg,
            &urls,
            cfg.query.clone(),
            &service_context,
            None,
            caller_auth_snapshot.as_ref(),
        )
        .await
        .map_err(|e| logged_internal_error("extract.start", e.as_ref()))?;
        Ok(AxonToolResponse::ok(
            "extract",
            "start",
            serde_json::json!({ "job_id": outcome.result.job_id }),
        ))
    }

    pub(super) async fn handle_extract(
        &self,
        req: ExtractRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        match req.subaction.unwrap_or(ExtractSubaction::Start) {
            ExtractSubaction::Start => self.handle_extract_start(req).await,
        }
    }
}
