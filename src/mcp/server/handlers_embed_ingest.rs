use super::AxonMcpServer;
use super::common::{
    InlineHint, invalid_params, logged_internal_error, parse_job_id, parse_limit, parse_offset,
    respond_with_mode, validate_mcp_embed_input_with_config,
};
use crate::mcp::schema::{
    AxonToolResponse, EmbedRequest, EmbedSubaction, IngestRequest, IngestSubaction, ResponseMode,
};
use crate::services::embed::{
    embed_cancel, embed_cleanup, embed_clear, embed_list, embed_recover, embed_start_with_context,
    embed_status,
};
use crate::services::ingest::{
    ingest_cancel, ingest_cleanup, ingest_clear, ingest_list, ingest_recover,
    ingest_start_with_context, ingest_status, source_from_mcp_request,
};
use rmcp::ErrorData;

impl AxonMcpServer {
    async fn handle_embed_start(
        &self,
        input: Option<String>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let input = input.ok_or_else(|| invalid_params("input is required for embed.start"))?;
        let input = validate_mcp_embed_input_with_config(self.cfg.as_ref(), &input)?;
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("embed.start.context", e.as_ref()))?;
        let outcome =
            embed_start_with_context(self.cfg.as_ref(), &input, &service_context, None, None)
                .await
                .map_err(|e| logged_internal_error("embed.start", e.as_ref()))?;
        Ok(AxonToolResponse::ok(
            "embed",
            "start",
            serde_json::json!({ "job_id": outcome.result.job_id }),
        ))
    }

    async fn handle_embed_list(
        &self,
        limit: Option<i64>,
        offset: Option<usize>,
        response_mode: Option<ResponseMode>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let limit = parse_limit(limit, 20);
        let offset = parse_offset(offset);
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("embed.list.context", e.as_ref()))?;
        let jobs = embed_list(
            service_context.as_ref(),
            limit,
            i64::try_from(offset).unwrap_or(i64::MAX),
        )
        .await
        .map_err(|e| logged_internal_error("embed.list", e.as_ref()))?;
        respond_with_mode(
            "embed",
            "list",
            response_mode,
            "embed-list",
            serde_json::json!({ "jobs": jobs.payload, "limit": limit, "offset": offset }),
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_embed(
        &self,
        req: EmbedRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        match req.subaction.unwrap_or(EmbedSubaction::Start) {
            EmbedSubaction::Start => self.handle_embed_start(req.input).await,
            EmbedSubaction::Status => {
                let id = parse_job_id(req.job_id.as_deref())?;
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("embed.status.context", e.as_ref()))?;
                let job = embed_status(service_context.as_ref(), id)
                    .await
                    .map_err(|e| logged_internal_error("embed.status", e.as_ref()))?;
                respond_with_mode(
                    "embed",
                    "status",
                    response_mode,
                    &format!("embed-status-{id}"),
                    serde_json::json!({ "job": job.map(|j| j.payload) }),
                    InlineHint::Default,
                )
                .await
            }
            EmbedSubaction::Cancel => {
                let id = parse_job_id(req.job_id.as_deref())?;
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("embed.cancel.context", e.as_ref()))?;
                let canceled = embed_cancel(service_context.as_ref(), id)
                    .await
                    .map_err(|e| logged_internal_error("embed.cancel", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "embed",
                    "cancel",
                    serde_json::json!({ "job_id": id.to_string(), "canceled": canceled }),
                ))
            }
            EmbedSubaction::List => {
                self.handle_embed_list(req.limit, req.offset, response_mode)
                    .await
            }
            EmbedSubaction::Cleanup => {
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("embed.cleanup.context", e.as_ref()))?;
                let deleted = embed_cleanup(service_context.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("embed.cleanup", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "embed",
                    "cleanup",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            EmbedSubaction::Clear => {
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("embed.clear.context", e.as_ref()))?;
                let deleted = embed_clear(service_context.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("embed.clear", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "embed",
                    "clear",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            EmbedSubaction::Recover => {
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("embed.recover.context", e.as_ref()))?;
                let recovered = embed_recover(service_context.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("embed.recover", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "embed",
                    "recover",
                    serde_json::json!({ "recovered": recovered }),
                ))
            }
        }
    }

    async fn handle_ingest_start(&self, req: IngestRequest) -> Result<AxonToolResponse, ErrorData> {
        let source = source_from_mcp_request(&req, self.cfg.as_ref()).map_err(invalid_params)?;
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("ingest.start.context", e.as_ref()))?;
        let outcome = ingest_start_with_context(self.cfg.as_ref(), source, &service_context)
            .await
            .map_err(|e| logged_internal_error("ingest.start", e.as_ref()))?;
        Ok(AxonToolResponse::ok(
            "ingest",
            "start",
            serde_json::json!({ "job_id": outcome.result.job_id }),
        ))
    }

    async fn handle_ingest_list(
        &self,
        limit: Option<i64>,
        offset: Option<usize>,
        response_mode: Option<ResponseMode>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let limit = parse_limit(limit, 20);
        let offset = parse_offset(offset);
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("ingest.list.context", e.as_ref()))?;
        let offset_i64 = i64::try_from(offset).unwrap_or(i64::MAX);
        let result = ingest_list(service_context.as_ref(), limit, offset_i64)
            .await
            .map_err(|e| logged_internal_error("ingest.list", e.as_ref()))?;
        // Derive truncation from page fullness — avoids a separate count query
        // that would bypass the service context and fail in lite mode.
        let page_len = result.payload.as_array().map_or(0, |a| a.len() as i64);
        let truncated = page_len >= limit;
        respond_with_mode(
            "ingest",
            "list",
            response_mode,
            "ingest-list",
            serde_json::json!({
                "jobs": result.payload,
                "limit": limit,
                "offset": offset,
                "truncated": truncated,
            }),
            InlineHint::Default,
        )
        .await
    }

    pub(super) async fn handle_ingest(
        &self,
        req: IngestRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        match req.subaction.unwrap_or(IngestSubaction::Start) {
            IngestSubaction::Start => self.handle_ingest_start(req).await,
            IngestSubaction::Status => {
                let id = parse_job_id(req.job_id.as_deref())?;
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("ingest.status.context", e.as_ref()))?;
                let job = ingest_status(service_context.as_ref(), id)
                    .await
                    .map_err(|e| logged_internal_error("ingest.status", e.as_ref()))?;
                respond_with_mode(
                    "ingest",
                    "status",
                    response_mode,
                    &format!("ingest-status-{id}"),
                    serde_json::json!({ "job": job.map(|j| j.payload) }),
                    InlineHint::Default,
                )
                .await
            }
            IngestSubaction::Cancel => {
                let id = parse_job_id(req.job_id.as_deref())?;
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("ingest.cancel.context", e.as_ref()))?;
                let canceled = ingest_cancel(service_context.as_ref(), id)
                    .await
                    .map_err(|e| logged_internal_error("ingest.cancel", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "ingest",
                    "cancel",
                    serde_json::json!({ "job_id": id.to_string(), "canceled": canceled }),
                ))
            }
            IngestSubaction::List => {
                self.handle_ingest_list(req.limit, req.offset, response_mode)
                    .await
            }
            IngestSubaction::Cleanup => {
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("ingest.cleanup.context", e.as_ref()))?;
                let deleted = ingest_cleanup(service_context.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("ingest.cleanup", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "ingest",
                    "cleanup",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            IngestSubaction::Clear => {
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("ingest.clear.context", e.as_ref()))?;
                let deleted = ingest_clear(service_context.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("ingest.clear", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "ingest",
                    "clear",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            IngestSubaction::Recover => {
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("ingest.recover.context", e.as_ref()))?;
                let recovered = ingest_recover(service_context.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("ingest.recover", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "ingest",
                    "recover",
                    serde_json::json!({ "recovered": recovered }),
                ))
            }
        }
    }
}

#[cfg(test)]
#[path = "handlers_embed_ingest_tests.rs"]
mod tests;
