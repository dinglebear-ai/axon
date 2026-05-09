use std::error::Error;

use crate::mcp::schema::{AxonRequest, CrawlRequest, CrawlSubaction};
use crate::services::context::ServiceContext;
use crate::services::crawl as crawl_svc;
use crate::services::system;
use crate::services::types::ClientActionError;
use uuid::Uuid;

pub async fn dispatch_action(
    service_context: &ServiceContext,
    action: AxonRequest,
) -> Result<serde_json::Value, ClientActionError> {
    match action {
        AxonRequest::Status(_) => {
            let result = system::full_status(service_context)
                .await
                .map_err(internal_error)?;
            Ok(result.payload)
        }
        AxonRequest::Crawl(req) => dispatch_crawl(service_context, req).await,
        other => Err(unsupported_action(action_name(&other))),
    }
}

async fn dispatch_crawl(
    service_context: &ServiceContext,
    req: CrawlRequest,
) -> Result<serde_json::Value, ClientActionError> {
    let subaction = match req.subaction {
        Some(subaction) => subaction,
        None => CrawlSubaction::Start,
    };
    match subaction {
        CrawlSubaction::Status => {
            let id = parse_job_id(req.job_id.as_deref())?;
            let result = crawl_svc::crawl_status(service_context, id)
                .await
                .map_err(internal_error)?;
            Ok(serde_json::json!({
                "job": result.payload,
                "output_files": result.output_files,
            }))
        }
        CrawlSubaction::List => {
            let limit = match req.limit {
                Some(limit) => limit.clamp(1, 500),
                None => 20,
            };
            let offset = match req.offset {
                Some(offset) => offset.min(i64::MAX as usize) as i64,
                None => 0,
            };
            let result = crawl_svc::crawl_list(service_context, limit, offset)
                .await
                .map_err(internal_error)?;
            Ok(serde_json::json!({
                "jobs": result.payload,
                "limit": limit,
                "offset": offset,
            }))
        }
        CrawlSubaction::Cancel => {
            let id = parse_job_id(req.job_id.as_deref())?;
            let canceled = crawl_svc::crawl_cancel(service_context, id)
                .await
                .map_err(internal_error)?;
            Ok(serde_json::json!({
                "job_id": id.to_string(),
                "canceled": canceled,
            }))
        }
        CrawlSubaction::Cleanup => {
            let deleted = crawl_svc::crawl_cleanup(service_context)
                .await
                .map_err(internal_error)?;
            Ok(serde_json::json!({ "deleted": deleted }))
        }
        CrawlSubaction::Clear => {
            let deleted = crawl_svc::crawl_clear(service_context)
                .await
                .map_err(internal_error)?;
            Ok(serde_json::json!({ "deleted": deleted }))
        }
        CrawlSubaction::Recover => {
            let recovered = crawl_svc::crawl_recover(service_context)
                .await
                .map_err(internal_error)?;
            Ok(serde_json::json!({ "recovered": recovered }))
        }
        CrawlSubaction::Start => Err(ClientActionError::new(
            "unsupported_action",
            "crawl.start is not exposed by the first-party action API yet",
            false,
            Some("use crawl lifecycle read actions until command migration is implemented".into()),
        )),
    }
}

fn parse_job_id(raw: Option<&str>) -> Result<Uuid, ClientActionError> {
    let raw = raw.ok_or_else(|| {
        ClientActionError::new(
            "invalid_request",
            "job_id is required",
            false,
            Some("include a UUID job_id for this lifecycle action".to_string()),
        )
    })?;
    Uuid::parse_str(raw).map_err(|err| {
        ClientActionError::new(
            "invalid_request",
            format!("invalid job_id: {err}"),
            false,
            Some("job_id must be a UUID returned by a start action".to_string()),
        )
    })
}

fn unsupported_action(action: &'static str) -> ClientActionError {
    ClientActionError::new(
        "unsupported_action",
        format!("{action} is not supported by the first-party action API yet"),
        false,
        Some("call /v1/capabilities to discover supported actions".to_string()),
    )
}

fn internal_error(err: Box<dyn Error>) -> ClientActionError {
    ClientActionError::new("internal", err.to_string(), true, None)
}

fn action_name(action: &AxonRequest) -> &'static str {
    match action {
        AxonRequest::Status(_) => "status",
        AxonRequest::Crawl(_) => "crawl",
        AxonRequest::Extract(_) => "extract",
        AxonRequest::Embed(_) => "embed",
        AxonRequest::Ingest(_) => "ingest",
        AxonRequest::Query(_) => "query",
        AxonRequest::Retrieve(_) => "retrieve",
        AxonRequest::Search(_) => "search",
        AxonRequest::Map(_) => "map",
        AxonRequest::Evaluate(_) => "evaluate",
        AxonRequest::Suggest(_) => "suggest",
        AxonRequest::Doctor(_) => "doctor",
        AxonRequest::Domains(_) => "domains",
        AxonRequest::Sources(_) => "sources",
        AxonRequest::Stats(_) => "stats",
        AxonRequest::Help(_) => "help",
        AxonRequest::Artifacts(_) => "artifacts",
        AxonRequest::Scrape(_) => "scrape",
        AxonRequest::Research(_) => "research",
        AxonRequest::Ask(_) => "ask",
        AxonRequest::Screenshot(_) => "screenshot",
        AxonRequest::ElicitDemo(_) => "elicit_demo",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
    use crate::mcp::schema::{CrawlRequest, CrawlSubaction, StatusRequest};
    use crate::services::runtime::ServiceJobRuntime;
    use crate::services::types::ServiceJob;
    use async_trait::async_trait;
    use std::sync::Arc;

    struct EmptyRuntime;

    #[async_trait]
    impl ServiceJobRuntime for EmptyRuntime {
        fn mode_name(&self) -> &'static str {
            "test"
        }

        async fn enqueue(&self, _payload: JobPayload) -> BackendResult<Uuid> {
            Err("not implemented".into())
        }

        async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
            Err("not implemented".into())
        }

        async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
            Ok(None)
        }

        async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
            Ok(false)
        }

        async fn list_jobs(
            &self,
            _kind: JobKind,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
            Ok(Vec::new())
        }

        async fn job_status(
            &self,
            _kind: JobKind,
            _id: Uuid,
        ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> {
            Ok(None)
        }

        async fn cancel_job(
            &self,
            _kind: JobKind,
            _id: Uuid,
        ) -> Result<bool, Box<dyn Error + Send + Sync>> {
            Ok(false)
        }

        async fn cleanup_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn clear_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn recover_jobs(
            &self,
            _kind: JobKind,
            _stale_threshold_ms: i64,
        ) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn count_jobs(&self, _kind: JobKind) -> Result<i64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }
    }

    fn test_context() -> ServiceContext {
        ServiceContext::from_runtime(
            Arc::new(crate::core::config::Config::default()),
            Arc::new(EmptyRuntime),
        )
    }

    #[tokio::test]
    async fn services_action_api_dispatches_status() {
        let result = dispatch_action(
            &test_context(),
            AxonRequest::Status(StatusRequest {
                subaction: None,
                response_mode: None,
            }),
        )
        .await;
        let result = match result {
            Ok(result) => result,
            Err(err) => panic!("status dispatch failed: {err:?}"),
        };

        assert_eq!(result["totals"]["crawl"], 0);
        assert!(result.get("local_crawl_jobs").is_some());
    }

    #[tokio::test]
    async fn services_action_api_dispatches_crawl_list_lifecycle() {
        let result = dispatch_action(
            &test_context(),
            AxonRequest::Crawl(CrawlRequest {
                subaction: Some(CrawlSubaction::List),
                urls: None,
                job_id: None,
                limit: Some(5),
                offset: Some(2),
                response_mode: None,
                max_pages: None,
                max_depth: None,
                include_subdomains: None,
                respect_robots: None,
                discover_sitemaps: None,
                sitemap_since_days: None,
                render_mode: None,
                delay_ms: None,
            }),
        )
        .await;
        let result = match result {
            Ok(result) => result,
            Err(err) => panic!("crawl list dispatch failed: {err:?}"),
        };

        assert_eq!(result["limit"], 5);
        assert_eq!(result["offset"], 2);
        assert_eq!(result["jobs"], serde_json::json!([]));
    }
}
