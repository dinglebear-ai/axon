use super::AxonMcpServer;
use super::common::{
    invalid_params, logged_internal_error, parse_job_id, parse_limit, parse_offset,
    respond_with_mode,
};
use crate::crates::mcp::schema::{
    AxonToolResponse, EmbedRequest, EmbedSubaction, IngestRequest, IngestSourceType,
    IngestSubaction, SessionsIngestOptions,
};
use crate::crates::services::ingest::IngestSource;
use rmcp::ErrorData;

fn parse_ingest_source(
    req: &mut IngestRequest,
    cfg: &crate::crates::core::config::Config,
) -> Result<IngestSource, ErrorData> {
    let source_type = req
        .source_type
        .take()
        .ok_or_else(|| invalid_params("source_type is required for ingest.start"))?;
    match source_type {
        IngestSourceType::Github => {
            let repo = req
                .target
                .take()
                .ok_or_else(|| invalid_params("target repo is required for github ingest"))?;
            Ok(IngestSource::Github {
                repo,
                include_source: req.include_source.unwrap_or(cfg.github_include_source),
            })
        }
        IngestSourceType::Reddit => {
            let target = req
                .target
                .take()
                .ok_or_else(|| invalid_params("target is required for reddit ingest"))?;
            Ok(IngestSource::Reddit { target })
        }
        IngestSourceType::Youtube => {
            let target = req
                .target
                .take()
                .ok_or_else(|| invalid_params("target is required for youtube ingest"))?;
            Ok(IngestSource::Youtube { target })
        }
        IngestSourceType::Sessions => {
            let sessions = req.sessions.take().unwrap_or(SessionsIngestOptions {
                claude: None,
                codex: None,
                gemini: None,
                project: None,
            });
            Ok(IngestSource::Sessions {
                sessions_claude: sessions.claude.unwrap_or(false),
                sessions_codex: sessions.codex.unwrap_or(false),
                sessions_gemini: sessions.gemini.unwrap_or(false),
                sessions_project: sessions.project,
            })
        }
    }
}

impl AxonMcpServer {
    async fn handle_embed_start(
        &self,
        input: Option<String>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let input = input.ok_or_else(|| invalid_params("input is required for embed.start"))?;
        let result =
            crate::crates::services::embed::embed_start_with_input(self.cfg.as_ref(), &input, None)
                .await
                .map_err(|e| logged_internal_error("embed.start", e))?;
        Ok(AxonToolResponse::ok(
            "embed",
            "start",
            serde_json::json!({ "job_id": result.job_id }),
        ))
    }

    async fn handle_embed_list(
        &self,
        limit: Option<i64>,
        offset: Option<usize>,
        response_mode: Option<crate::crates::mcp::schema::ResponseMode>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let limit = parse_limit(limit, 20);
        let offset = parse_offset(offset);
        let jobs =
            crate::crates::services::embed::embed_list(self.cfg.as_ref(), limit, offset as i64)
                .await
                .map_err(|e| logged_internal_error("embed.list", e))?;
        respond_with_mode(
            "embed",
            "list",
            response_mode,
            "embed-list",
            serde_json::json!({ "jobs": jobs.payload, "limit": limit, "offset": offset }),
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
                let id = parse_job_id(req.job_id.as_ref())?;
                let job = crate::crates::services::embed::embed_status(self.cfg.as_ref(), id)
                    .await
                    .map_err(|e| logged_internal_error("embed.status", e))?;
                respond_with_mode(
                    "embed",
                    "status",
                    response_mode,
                    &format!("embed-status-{id}"),
                    serde_json::json!({ "job": job.map(|j| j.payload) }),
                )
                .await
            }
            EmbedSubaction::Cancel => {
                let id = parse_job_id(req.job_id.as_ref())?;
                let canceled = crate::crates::services::embed::embed_cancel(self.cfg.as_ref(), id)
                    .await
                    .map_err(|e| logged_internal_error("embed.cancel", e))?;
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
                let deleted = crate::crates::services::embed::embed_cleanup(self.cfg.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("embed.cleanup", e))?;
                Ok(AxonToolResponse::ok(
                    "embed",
                    "cleanup",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            EmbedSubaction::Clear => {
                let deleted = crate::crates::services::embed::embed_clear(self.cfg.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("embed.clear", e))?;
                Ok(AxonToolResponse::ok(
                    "embed",
                    "clear",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            EmbedSubaction::Recover => {
                let recovered = crate::crates::services::embed::embed_recover(self.cfg.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("embed.recover", e))?;
                Ok(AxonToolResponse::ok(
                    "embed",
                    "recover",
                    serde_json::json!({ "recovered": recovered }),
                ))
            }
        }
    }

    async fn handle_ingest_start(
        &self,
        req: &mut IngestRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let source = parse_ingest_source(req, self.cfg.as_ref())?;
        let result = crate::crates::services::ingest::ingest_start(self.cfg.as_ref(), source)
            .await
            .map_err(|e| logged_internal_error("ingest.start", e))?;
        Ok(AxonToolResponse::ok(
            "ingest",
            "start",
            serde_json::json!({ "job_id": result.job_id }),
        ))
    }

    async fn handle_ingest_list(
        &self,
        limit: Option<i64>,
        offset: Option<usize>,
        response_mode: Option<crate::crates::mcp::schema::ResponseMode>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let limit = parse_limit(limit, 20);
        let offset = parse_offset(offset);
        let jobs =
            crate::crates::services::ingest::ingest_list(self.cfg.as_ref(), limit, offset as i64)
                .await
                .map_err(|e| logged_internal_error("ingest.list", e))?;
        respond_with_mode(
            "ingest",
            "list",
            response_mode,
            "ingest-list",
            serde_json::json!({ "jobs": jobs.payload, "limit": limit, "offset": offset }),
        )
        .await
    }

    pub(super) async fn handle_ingest(
        &self,
        mut req: IngestRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        match req.subaction.unwrap_or(IngestSubaction::Start) {
            IngestSubaction::Start => self.handle_ingest_start(&mut req).await,
            IngestSubaction::Status => {
                let id = parse_job_id(req.job_id.as_ref())?;
                let job = crate::crates::services::ingest::ingest_status(self.cfg.as_ref(), id)
                    .await
                    .map_err(|e| logged_internal_error("ingest.status", e))?;
                respond_with_mode(
                    "ingest",
                    "status",
                    response_mode,
                    &format!("ingest-status-{id}"),
                    serde_json::json!({ "job": job.map(|j| j.payload) }),
                )
                .await
            }
            IngestSubaction::Cancel => {
                let id = parse_job_id(req.job_id.as_ref())?;
                let canceled =
                    crate::crates::services::ingest::ingest_cancel(self.cfg.as_ref(), id)
                        .await
                        .map_err(|e| logged_internal_error("ingest.cancel", e))?;
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
                let deleted = crate::crates::services::ingest::ingest_cleanup(self.cfg.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("ingest.cleanup", e))?;
                Ok(AxonToolResponse::ok(
                    "ingest",
                    "cleanup",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            IngestSubaction::Clear => {
                let deleted = crate::crates::services::ingest::ingest_clear(self.cfg.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("ingest.clear", e))?;
                Ok(AxonToolResponse::ok(
                    "ingest",
                    "clear",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            IngestSubaction::Recover => {
                let recovered = crate::crates::services::ingest::ingest_recover(self.cfg.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("ingest.recover", e))?;
                Ok(AxonToolResponse::ok(
                    "ingest",
                    "recover",
                    serde_json::json!({ "recovered": recovered }),
                ))
            }
        }
    }
}
