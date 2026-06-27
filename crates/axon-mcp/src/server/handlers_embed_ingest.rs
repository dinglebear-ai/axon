use super::AxonMcpServer;
use super::common::{
    InlineHint, invalid_params, logged_internal_error, parse_job_id, respond_with_mode,
    validate_mcp_collection, validate_mcp_embed_input_with_config,
};
use crate::schema::{
    AxonToolResponse, EmbedRequest, EmbedSubaction, IngestRequest, IngestSubaction, ResponseMode,
};
use axon_services::embed::{
    embed_cancel, embed_cleanup, embed_clear, embed_input_is_local_path, embed_list,
    embed_now_with_source, embed_recover, embed_start_with_context, embed_status,
};
use axon_services::ingest::{
    ingest_cancel, ingest_cleanup, ingest_clear, ingest_list, ingest_recover,
    ingest_start_with_context, ingest_status, source_from_mcp_request,
};
use rmcp::ErrorData;

impl AxonMcpServer {
    async fn handle_embed_start(
        &self,
        input: Option<String>,
        source_type: Option<String>,
        collection: Option<String>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let input = input.ok_or_else(|| invalid_params("input is required for embed.start"))?;
        let input = validate_mcp_embed_input_with_config(self.cfg.as_ref(), &input)?;
        let collection = collection
            .as_deref()
            .map(validate_mcp_collection)
            .transpose()?;
        let cfg = self
            .cfg
            .apply_overrides(&axon_core::config::ConfigOverrides {
                collection,
                ..axon_core::config::ConfigOverrides::default()
            });
        // A local path must be embedded in-process by a process that shares its
        // filesystem. Enqueuing it lets another worker (e.g. the axon container)
        // claim a path it cannot read. This matches the intent of the CLI guard in
        // crates/axon-cli/src/commands/embed.rs. `validate_mcp_embed_input_with_config`
        // (called earlier) already rejected a path-like input that does not exist,
        // so a true here is a path visible to this process; URL / free-text inputs
        // fall through to the queue.
        if embed_input_is_local_path(&input) {
            let result = embed_now_with_source(&cfg, &input, source_type.as_deref())
                .await
                .map_err(|e| logged_internal_error("embed.start", e.as_ref()))?;
            // Surface the real embed counts (docs_embedded / docs_failed /
            // chunks_embedded) from the in-process result so a partial embed is
            // not silently reported as a clean success.
            let mut data = result.payload;
            if let Some(obj) = data.as_object_mut() {
                obj.insert("status".to_string(), serde_json::json!("completed"));
            }
            return Ok(AxonToolResponse::ok("embed", "start", data));
        }
        let service_context = self
            .service_context_for(cfg.clone())
            .await
            .map_err(|e| logged_internal_error("embed.start.context", e.as_ref()))?;
        let outcome =
            embed_start_with_context(&cfg, &input, &service_context, None, source_type.as_deref())
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
        let (limit, offset) = axon_services::transport::job_list_pagination(limit, offset);
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("embed.list.context", e.as_ref()))?;
        let jobs = embed_list(service_context.as_ref(), limit, offset)
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
            EmbedSubaction::Start => {
                self.handle_embed_start(req.input, req.source_type, req.collection)
                    .await
            }
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
        let (limit, offset) = axon_services::transport::job_list_pagination(limit, offset);
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("ingest.list.context", e.as_ref()))?;
        let result = ingest_list(service_context.as_ref(), limit, offset)
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
