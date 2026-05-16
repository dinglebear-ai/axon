mod commands;

use std::error::Error;

use crate::mcp::schema::{
    AxonRequest, CrawlSubaction, EmbedSubaction, ExtractSubaction, IngestSubaction, SetupMode,
    WatchSubaction,
};
use crate::services::context::ServiceContext;
use crate::services::system;
use crate::services::types::ClientActionError;

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
        AxonRequest::Crawl(req) => commands::dispatch_crawl(service_context, req).await,
        AxonRequest::Extract(req) => commands::dispatch_extract(service_context, req).await,
        AxonRequest::Embed(req) => commands::dispatch_embed(service_context, req).await,
        AxonRequest::Ingest(req) => commands::dispatch_ingest(service_context, req).await,
        AxonRequest::Scrape(req) => commands::dispatch_scrape(service_context, req).await,
        AxonRequest::Screenshot(req) => commands::dispatch_screenshot(service_context, req).await,
        other => Err(unsupported_action(action_name(&other))),
    }
}

pub fn required_scope(action: &AxonRequest) -> Option<&'static str> {
    match action {
        AxonRequest::Status(_) => Some("axon:read"),
        AxonRequest::Crawl(req) => match req.subaction.unwrap_or(CrawlSubaction::Start) {
            CrawlSubaction::Status | CrawlSubaction::List => Some("axon:read"),
            CrawlSubaction::Start
            | CrawlSubaction::Cancel
            | CrawlSubaction::Cleanup
            | CrawlSubaction::Clear
            | CrawlSubaction::Recover => Some("axon:write"),
        },
        AxonRequest::Extract(req) => match req.subaction.unwrap_or(ExtractSubaction::Start) {
            ExtractSubaction::Status | ExtractSubaction::List => Some("axon:read"),
            _ => Some("axon:write"),
        },
        AxonRequest::Embed(req) => match req.subaction.unwrap_or(EmbedSubaction::Start) {
            EmbedSubaction::Status | EmbedSubaction::List => Some("axon:read"),
            _ => Some("axon:write"),
        },
        AxonRequest::Ingest(req) => match req.subaction.unwrap_or(IngestSubaction::Start) {
            IngestSubaction::Status | IngestSubaction::List => Some("axon:read"),
            _ => Some("axon:write"),
        },
        AxonRequest::Query(_)
        | AxonRequest::Retrieve(_)
        | AxonRequest::Search(_)
        | AxonRequest::Map(_)
        | AxonRequest::Evaluate(_)
        | AxonRequest::Suggest(_)
        | AxonRequest::Doctor(_)
        | AxonRequest::Domains(_)
        | AxonRequest::Sources(_)
        | AxonRequest::Stats(_)
        | AxonRequest::Help(_)
        | AxonRequest::Artifacts(_)
        | AxonRequest::Research(_)
        | AxonRequest::Ask(_)
        | AxonRequest::Debug(_) => Some("axon:read"),
        AxonRequest::Dedupe(_) | AxonRequest::Migrate(_) => Some("axon:write"),
        AxonRequest::Watch(req) => match req.subaction.unwrap_or(WatchSubaction::List) {
            WatchSubaction::List | WatchSubaction::Get | WatchSubaction::History => {
                Some("axon:read")
            }
            WatchSubaction::Create | WatchSubaction::RunNow => Some("axon:write"),
        },
        AxonRequest::Setup(req) => match req.mode.unwrap_or(SetupMode::Check) {
            SetupMode::Check => Some("axon:read"),
            SetupMode::FirstRun | SetupMode::Repair | SetupMode::MigrateEnv => Some("axon:write"),
        },
        AxonRequest::Scrape(_) | AxonRequest::Screenshot(_) => Some("axon:write"),
        _ => None,
    }
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
        AxonRequest::Debug(_) => "debug",
        AxonRequest::Dedupe(_) => "dedupe",
        AxonRequest::Migrate(_) => "migrate",
        AxonRequest::Watch(_) => "watch",
        AxonRequest::Setup(_) => "setup",
        AxonRequest::ElicitDemo(_) => "elicit_demo",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::Config;
    use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
    use crate::mcp::schema::{CrawlRequest, CrawlSubaction, StatusRequest};
    use crate::services::runtime::ServiceJobRuntime;
    use crate::services::types::ServiceJob;
    use async_trait::async_trait;
    use std::sync::Arc;
    use uuid::Uuid;

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
        ServiceContext::from_runtime(Arc::new(Config::default()), Arc::new(EmptyRuntime))
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
