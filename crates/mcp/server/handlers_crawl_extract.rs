use super::AxonMcpServer;
use super::common::{
    apply_crawl_overrides, invalid_params, logged_internal_error, parse_job_id, parse_limit,
    parse_offset, respond_with_mode,
};
use crate::crates::core::http::validate_url;
use crate::crates::mcp::schema::{
    AxonToolResponse, CrawlRequest, CrawlSubaction, ExtractRequest, ExtractSubaction,
};
use crate::crates::services::crawl as crawl_svc;
use crate::crates::services::extract as extract_svc;
use rmcp::ErrorData;

impl AxonMcpServer {
    async fn handle_crawl_start(
        &self,
        cfg: &crate::crates::core::config::Config,
        urls: Option<Vec<String>>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let urls = urls.ok_or_else(|| invalid_params("urls is required for crawl.start"))?;
        if urls.is_empty() {
            return Err(invalid_params("urls cannot be empty"));
        }
        for url in &urls {
            validate_url(url).map_err(|e| invalid_params(e.to_string()))?;
        }
        let result = crawl_svc::crawl_start(cfg, &urls, None)
            .await
            .map_err(|e| logged_internal_error("crawl.start", e))?;
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
        cfg: &crate::crates::core::config::Config,
        limit: Option<i64>,
        offset: Option<usize>,
        response_mode: Option<crate::crates::mcp::schema::ResponseMode>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let limit = parse_limit(limit, 20);
        let offset = parse_offset(offset);
        let jobs = crawl_svc::crawl_list(cfg, limit, offset as i64)
            .await
            .map_err(|e| logged_internal_error("crawl.list", e))?;
        respond_with_mode(
            "crawl",
            "list",
            response_mode,
            "crawl-list",
            serde_json::json!({ "jobs": jobs.payload, "limit": limit, "offset": offset }),
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
        for url in &urls {
            validate_url(url).map_err(|e| invalid_params(e.to_string()))?;
        }
        let mut cfg = self.cfg.as_ref().clone();
        cfg.query = prompt;
        if let Some(mp) = max_pages {
            cfg.max_pages = mp;
        }
        let result = extract_svc::extract_start(&cfg, &urls, None)
            .await
            .map_err(|e| logged_internal_error("extract.start", e))?;
        Ok(AxonToolResponse::ok(
            "extract",
            "start",
            serde_json::json!({ "job_id": result.job_id }),
        ))
    }

    async fn handle_extract_list(
        &self,
        limit: Option<i64>,
        offset: Option<usize>,
        response_mode: Option<crate::crates::mcp::schema::ResponseMode>,
    ) -> Result<AxonToolResponse, ErrorData> {
        let limit = parse_limit(limit, 20);
        let offset = parse_offset(offset);
        let jobs = extract_svc::extract_list(self.cfg.as_ref(), limit, offset as i64)
            .await
            .map_err(|e| logged_internal_error("extract.list", e))?;
        respond_with_mode(
            "extract",
            "list",
            response_mode,
            "extract-list",
            serde_json::json!({ "jobs": jobs.payload, "limit": limit, "offset": offset }),
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
                let id = parse_job_id(req.job_id.as_ref())?;
                let result = crawl_svc::crawl_status(&cfg, id)
                    .await
                    .map_err(|e| logged_internal_error("crawl.status", e))?;
                let output_files = result.output_files;
                Ok(AxonToolResponse::ok(
                    "crawl",
                    "status",
                    serde_json::json!({
                        "job": result.payload,
                        "output_files": output_files,
                    }),
                ))
            }
            CrawlSubaction::Cancel => {
                let id = parse_job_id(req.job_id.as_ref())?;
                let canceled = crawl_svc::crawl_cancel(&cfg, id)
                    .await
                    .map_err(|e| logged_internal_error("crawl.cancel", e))?;
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
                let deleted = crawl_svc::crawl_cleanup(&cfg)
                    .await
                    .map_err(|e| logged_internal_error("crawl.cleanup", e))?;
                Ok(AxonToolResponse::ok(
                    "crawl",
                    "cleanup",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            CrawlSubaction::Clear => {
                let deleted = crawl_svc::crawl_clear(&cfg)
                    .await
                    .map_err(|e| logged_internal_error("crawl.clear", e))?;
                Ok(AxonToolResponse::ok(
                    "crawl",
                    "clear",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            CrawlSubaction::Recover => {
                let recovered = crawl_svc::crawl_recover(&cfg)
                    .await
                    .map_err(|e| logged_internal_error("crawl.recover", e))?;
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
                let id = parse_job_id(req.job_id.as_ref())?;
                let job = extract_svc::extract_status(self.cfg.as_ref(), id)
                    .await
                    .map_err(|e| logged_internal_error("extract.status", e))?;
                respond_with_mode(
                    "extract",
                    "status",
                    response_mode,
                    &format!("extract-status-{id}"),
                    serde_json::json!({ "job": job.map(|j| j.payload) }),
                )
                .await
            }
            ExtractSubaction::Cancel => {
                let id = parse_job_id(req.job_id.as_ref())?;
                let canceled = extract_svc::extract_cancel(self.cfg.as_ref(), id)
                    .await
                    .map_err(|e| logged_internal_error("extract.cancel", e))?;
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
                let deleted = extract_svc::extract_cleanup(self.cfg.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("extract.cleanup", e))?;
                Ok(AxonToolResponse::ok(
                    "extract",
                    "cleanup",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            ExtractSubaction::Clear => {
                let deleted = extract_svc::extract_clear(self.cfg.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("extract.clear", e))?;
                Ok(AxonToolResponse::ok(
                    "extract",
                    "clear",
                    serde_json::json!({ "deleted": deleted }),
                ))
            }
            ExtractSubaction::Recover => {
                let recovered = extract_svc::extract_recover(self.cfg.as_ref())
                    .await
                    .map_err(|e| logged_internal_error("extract.recover", e))?;
                Ok(AxonToolResponse::ok(
                    "extract",
                    "recover",
                    serde_json::json!({ "recovered": recovered }),
                ))
            }
        }
    }
}
