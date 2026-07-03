use super::AxonMcpServer;
use super::common::{
    InlineHint, apply_extract_overrides, invalid_params, logged_internal_error, parse_job_id,
    respond_with_mode, validate_mcp_urls,
};
use crate::schema::{AxonToolResponse, ExtractRequest, ExtractSubaction, ResponseMode};
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
        let outcome = extract_svc::extract_start_with_context(
            &cfg,
            &urls,
            cfg.query.clone(),
            &service_context,
            None,
        )
        .await
        .map_err(|e| logged_internal_error("extract.start", e.as_ref()))?;
        Ok(AxonToolResponse::ok(
            "extract",
            "start",
            serde_json::json!({ "job_id": outcome.result.job_id }),
        ))
    }

    async fn handle_extract_list(
        &self,
        limit: Option<i64>,
        offset: Option<usize>,
        response_mode: Option<ResponseMode>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let (limit, offset) = axon_services::transport::job_list_pagination(limit, offset);
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("extract.list.context", e.as_ref()))?;
        let jobs = extract_svc::extract_list(service_context.as_ref(), limit, offset)
            .await
            .map_err(|e| logged_internal_error("extract.list", e.as_ref()))?;
        respond_with_mode(
            "extract",
            "list",
            response_mode,
            "extract-list",
            serde_json::json!({ "jobs": jobs.payload, "limit": limit, "offset": offset }),
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_extract(
        &self,
        req: ExtractRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        match req.subaction.unwrap_or(ExtractSubaction::Start) {
            ExtractSubaction::Start => self.handle_extract_start(req).await,
            ExtractSubaction::Status => {
                let id = parse_job_id(req.job_id.as_deref())?;
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("extract.status.context", e.as_ref()))?;
                let job = extract_svc::extract_status(service_context.as_ref(), id)
                    .await
                    .map_err(|e| logged_internal_error("extract.status", e.as_ref()))?;
                let payload = job.map(|j| j.payload);
                let progress = payload.as_ref().map(|p| {
                    axon_api::job_progress::JobProgress::from_wire_value(
                        axon_api::job_progress::JobFamily::Extract,
                        p,
                    )
                });
                respond_with_mode(
                    "extract",
                    "status",
                    response_mode,
                    &format!("extract-status-{id}"),
                    serde_json::json!({ "job": payload, "progress": progress }),
                    InlineHint::Default,
                )
                .await
            }
            ExtractSubaction::Cancel => {
                let id = parse_job_id(req.job_id.as_deref())?;
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("extract.cancel.context", e.as_ref()))?;
                let canceled = extract_svc::extract_cancel(service_context.as_ref(), id)
                    .await
                    .map_err(|e| logged_internal_error("extract.cancel", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "extract",
                    "cancel",
                    serde_json::json!({ "job_id": id.to_string(), "canceled": canceled }),
                ))
            }
            ExtractSubaction::List => {
                self.handle_extract_list(req.limit, req.offset, response_mode)
                    .await
            }
            ExtractSubaction::Cleanup => {
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("extract.cleanup.context", e.as_ref()))?;
                let deleted = extract_svc::extract_cleanup(service_context.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("extract.cleanup", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "extract",
                    "cleanup",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            ExtractSubaction::Clear => {
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("extract.clear.context", e.as_ref()))?;
                let deleted = extract_svc::extract_clear(service_context.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("extract.clear", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "extract",
                    "clear",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            ExtractSubaction::Recover => {
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("extract.recover.context", e.as_ref()))?;
                let recovered = extract_svc::extract_recover(service_context.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("extract.recover", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "extract",
                    "recover",
                    serde_json::json!({ "recovered": recovered }),
                ))
            }
        }
    }
}
