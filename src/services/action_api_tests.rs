use super::*;
use crate::core::config::Config;
use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
use crate::mcp::schema::{
    AskRequest, CrawlRequest, CrawlSubaction, DedupeRequest, EvaluateRequest, ResearchRequest,
    StatusRequest, SuggestRequest,
};
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

#[test]
fn required_scope_uses_secure_defaults_and_promotes_llm_actions() {
    assert_eq!(
        required_scope(&AxonRequest::Ask(AskRequest {
            query: Some("q".into()),
            graph: None,
            diagnostics: None,
            explain: None,
            collection: None,
            since: None,
            before: None,
            hybrid_search: None,
            response_mode: None,
        })),
        Some("axon:write")
    );
    assert_eq!(
        required_scope(&AxonRequest::Research(ResearchRequest {
            query: Some("q".into()),
            limit: None,
            offset: None,
            search_time_range: None,
            response_mode: None,
        })),
        Some("axon:write")
    );
    assert_eq!(
        required_scope(&AxonRequest::Evaluate(EvaluateRequest {
            query: Some("q".into()),
            diagnostics: None,
            retrieval_ab: None,
            collection: None,
            since: None,
            before: None,
            hybrid_search: None,
            response_mode: None,
        })),
        Some("axon:write")
    );
    assert_eq!(
        required_scope(&AxonRequest::Suggest(SuggestRequest {
            focus: None,
            limit: None,
            collection: None,
            response_mode: None,
        })),
        Some("axon:write")
    );
    assert_eq!(
        required_scope(&AxonRequest::Dedupe(DedupeRequest {
            collection: None,
            response_mode: None,
        })),
        Some("axon:write")
    );
}
