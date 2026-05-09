use super::AxonMcpServer;
use super::common::{
    InlineHint, apply_crawl_overrides, invalid_params, logged_internal_error, parse_job_id,
    parse_limit, parse_offset, respond_with_mode, validate_mcp_urls,
};
use crate::core::config::{Config, ConfigOverrides};
use crate::mcp::schema::{
    AxonToolResponse, CrawlRequest, CrawlSubaction, ExtractRequest, ExtractSubaction, ResponseMode,
};
use crate::services::crawl as crawl_svc;
use crate::services::extract as extract_svc;
use rmcp::ErrorData;

impl AxonMcpServer {
    async fn handle_crawl_start(
        &self,
        cfg: &Config,
        urls: Option<Vec<String>>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let urls = urls.ok_or_else(|| invalid_params("urls is required for crawl.start"))?;
        if urls.is_empty() {
            return Err(invalid_params("urls cannot be empty"));
        }
        validate_mcp_urls(&urls)?;
        let service_context = self
            .service_context_for(cfg.clone())
            .await
            .map_err(|e| logged_internal_error("crawl.start.context", e.as_ref()))?;
        let outcome = crawl_svc::crawl_start_with_context(cfg, &urls, &service_context, None)
            .await
            .map_err(|e| logged_internal_error("crawl.start", e.as_ref()))?;
        let result = outcome.result;
        let job_ids = result.job_ids;
        let output_dir = result.output_dir;
        let predicted_paths = result.predicted_paths;
        let jobs = result
            .jobs
            .into_iter()
            .map(|job| {
                serde_json::json!({
                    "job_id": job.job_id,
                    "url": job.url,
                    "output_dir": job.output_dir,
                    "predicted_paths": job.predicted_paths,
                })
            })
            .collect::<Vec<_>>();
        Ok(AxonToolResponse::ok(
            "crawl",
            "start",
            serde_json::json!({
                "job_ids": job_ids,
                "output_dir": output_dir,
                "predicted_paths": predicted_paths,
                "jobs": jobs
            }),
        ))
    }

    async fn handle_crawl_list(
        &self,
        cfg: &Config,
        limit: Option<i64>,
        offset: Option<usize>,
        response_mode: Option<ResponseMode>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let limit = parse_limit(limit, 20);
        let offset = parse_offset(offset);
        let service_context = self
            .service_context_for(cfg.clone())
            .await
            .map_err(|e| logged_internal_error("crawl.list.context", e.as_ref()))?;
        let jobs = crawl_svc::crawl_list(
            &service_context,
            limit,
            i64::try_from(offset).unwrap_or(i64::MAX),
        )
        .await
        .map_err(|e| logged_internal_error("crawl.list", e.as_ref()))?;
        respond_with_mode(
            "crawl",
            "list",
            response_mode,
            "crawl-list",
            serde_json::json!({ "jobs": jobs.payload, "limit": limit, "offset": offset }),
            InlineHint::Default,
        )
        .await
    }

    async fn handle_extract_start(
        &self,
        urls: Option<Vec<String>>,
        prompt: Option<String>,
        max_pages: Option<u32>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let urls = urls.ok_or_else(|| invalid_params("urls is required for extract.start"))?;
        if urls.is_empty() {
            return Err(invalid_params("urls cannot be empty"));
        }
        validate_mcp_urls(&urls)?;
        let cfg = self.cfg.apply_overrides(&ConfigOverrides {
            query: Some(prompt),
            max_pages,
            ..ConfigOverrides::default()
        });
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
        let limit = parse_limit(limit, 20);
        let offset = parse_offset(offset);
        let service_context = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("extract.list.context", e.as_ref()))?;
        let jobs = extract_svc::extract_list(
            service_context.as_ref(),
            limit,
            i64::try_from(offset).unwrap_or(i64::MAX),
        )
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

    pub(super) async fn handle_crawl(
        &self,
        req: CrawlRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let cfg = apply_crawl_overrides(self.cfg.as_ref(), &req);
        let response_mode = req.response_mode;
        match req.subaction.unwrap_or(CrawlSubaction::Start) {
            CrawlSubaction::Start => self.handle_crawl_start(&cfg, req.urls).await,
            CrawlSubaction::Status => {
                let id = parse_job_id(req.job_id.as_deref())?;
                let service_context = self
                    .service_context_for(cfg.clone())
                    .await
                    .map_err(|e| logged_internal_error("crawl.status.context", e.as_ref()))?;
                let result = crawl_svc::crawl_status(&service_context, id)
                    .await
                    .map_err(|e| logged_internal_error("crawl.status", e.as_ref()))?;
                let output_files = result.output_files;
                let output_file_handles = result.output_file_handles;
                Ok(AxonToolResponse::ok(
                    "crawl",
                    "status",
                    serde_json::json!({
                        "job": result.payload,
                        "output_files": output_files,
                        "output_file_handles": output_file_handles,
                    }),
                ))
            }
            CrawlSubaction::Cancel => {
                let id = parse_job_id(req.job_id.as_deref())?;
                let service_context = self
                    .service_context_for(cfg.clone())
                    .await
                    .map_err(|e| logged_internal_error("crawl.cancel.context", e.as_ref()))?;
                let canceled = crawl_svc::crawl_cancel(&service_context, id)
                    .await
                    .map_err(|e| logged_internal_error("crawl.cancel", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "crawl",
                    "cancel",
                    serde_json::json!({ "job_id": id.to_string(), "canceled": canceled }),
                ))
            }
            CrawlSubaction::List => {
                self.handle_crawl_list(&cfg, req.limit, req.offset, response_mode)
                    .await
            }
            CrawlSubaction::Cleanup => {
                let service_context = self
                    .service_context_for(cfg.clone())
                    .await
                    .map_err(|e| logged_internal_error("crawl.cleanup.context", e.as_ref()))?;
                let deleted = crawl_svc::crawl_cleanup(&service_context)
                    .await
                    .map_err(|e| logged_internal_error("crawl.cleanup", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "crawl",
                    "cleanup",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            CrawlSubaction::Clear => {
                let service_context = self
                    .service_context_for(cfg.clone())
                    .await
                    .map_err(|e| logged_internal_error("crawl.clear.context", e.as_ref()))?;
                let deleted = crawl_svc::crawl_clear(&service_context)
                    .await
                    .map_err(|e| logged_internal_error("crawl.clear", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "crawl",
                    "clear",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            CrawlSubaction::Recover => {
                let service_context = self
                    .service_context_for(cfg.clone())
                    .await
                    .map_err(|e| logged_internal_error("crawl.recover.context", e.as_ref()))?;
                let recovered = crawl_svc::crawl_recover(&service_context)
                    .await
                    .map_err(|e| logged_internal_error("crawl.recover", e.as_ref()))?;
                Ok(AxonToolResponse::ok(
                    "crawl",
                    "recover",
                    serde_json::json!({ "recovered": recovered }),
                ))
            }
        }
    }

    pub(super) async fn handle_extract(
        &self,
        req: ExtractRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        match req.subaction.unwrap_or(ExtractSubaction::Start) {
            ExtractSubaction::Start => {
                self.handle_extract_start(req.urls, req.prompt, req.max_pages)
                    .await
            }
            ExtractSubaction::Status => {
                let id = parse_job_id(req.job_id.as_deref())?;
                let service_context = self
                    .base_service_context()
                    .await
                    .map_err(|e| logged_internal_error("extract.status.context", e.as_ref()))?;
                let job = extract_svc::extract_status(service_context.as_ref(), id)
                    .await
                    .map_err(|e| logged_internal_error("extract.status", e.as_ref()))?;
                respond_with_mode(
                    "extract",
                    "status",
                    response_mode,
                    &format!("extract-status-{id}"),
                    serde_json::json!({ "job": job.map(|j| j.payload) }),
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
