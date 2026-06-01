use super::*;
use crate::core::config::Config;
use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
use crate::mcp::schema::{
    AskRequest, CrawlRequest, CrawlSubaction, DedupeRequest, ElicitDemoRequest, EvaluateRequest,
    MigrateRequest, QueryRequest, ResearchRequest, StatusRequest, SuggestRequest,
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

    async fn count_jobs_by_status(
        &self,
        _kind: JobKind,
    ) -> Result<
        std::collections::HashMap<crate::jobs::status::JobStatus, i64>,
        Box<dyn Error + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
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
            discover_llms_txt: None,
            max_llms_txt_urls: None,
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

// ── required_scope invariant tests ────────────────────────────────────────────
// These directly verify the scope assignments that authorize_action depends on.
// Changing a scope here requires updating the breaking-change section of CHANGELOG.md.

fn req_ask() -> AxonRequest {
    AxonRequest::Ask(AskRequest {
        query: Some("test?".into()),
        diagnostics: None,
        explain: None,
        collection: None,
        since: None,
        before: None,
        hybrid_search: None,
        response_mode: None,
    })
}

fn req_research() -> AxonRequest {
    AxonRequest::Research(ResearchRequest {
        query: Some("test".into()),
        limit: None,
        offset: None,
        search_time_range: None,
        response_mode: None,
    })
}

fn req_evaluate() -> AxonRequest {
    AxonRequest::Evaluate(EvaluateRequest {
        query: Some("test?".into()),
        diagnostics: None,
        retrieval_ab: None,
        collection: None,
        since: None,
        before: None,
        hybrid_search: None,
        response_mode: None,
    })
}

fn req_suggest() -> AxonRequest {
    AxonRequest::Suggest(SuggestRequest {
        focus: None,
        limit: None,
        collection: None,
        response_mode: None,
    })
}

fn req_migrate() -> AxonRequest {
    AxonRequest::Migrate(MigrateRequest {
        from: Some("src".into()),
        to: Some("dst".into()),
        response_mode: None,
    })
}

fn req_dedupe() -> AxonRequest {
    AxonRequest::Dedupe(DedupeRequest {
        collection: None,
        response_mode: None,
    })
}

fn req_query() -> AxonRequest {
    AxonRequest::Query(QueryRequest {
        query: Some("test".into()),
        limit: None,
        offset: None,
        collection: None,
        since: None,
        before: None,
        hybrid_search: None,
        response_mode: None,
    })
}

#[test]
fn required_scope_ask_evaluate_suggest_research_are_write() {
    // F10: these trigger Gemini completions — must require axon:write.
    for req in [req_ask(), req_evaluate(), req_suggest(), req_research()] {
        assert_eq!(
            required_scope(&req),
            Some("axon:write"),
            "expected axon:write for {:?}",
            std::mem::discriminant(&req)
        );
    }
}

#[test]
fn required_scope_migrate_dedupe_are_write() {
    // F5 invariant: these must never return None — authorize_action's unconditional
    // auth guard for Migrate/Dedupe depends on required_scope returning Some(...).
    for req in [req_migrate(), req_dedupe()] {
        let scope = required_scope(&req);
        assert_eq!(
            scope,
            Some("axon:write"),
            "Migrate/Dedupe must return Some(axon:write) — None would bypass scope check: {:?}",
            std::mem::discriminant(&req)
        );
    }
}

#[test]
fn required_scope_elicit_demo_is_write() {
    // F1 / exhaustiveness: ElicitDemo is explicit in required_scope.
    // With no wildcard arm, the compiler enforces scope assignment for every future variant.
    let req = AxonRequest::ElicitDemo(ElicitDemoRequest {
        message: None,
        response_mode: None,
    });
    assert_eq!(
        required_scope(&req),
        Some("axon:write"),
        "ElicitDemo must return Some, never None"
    );
}

#[test]
fn required_scope_read_only_ops_are_read() {
    // Regression: query and similar read-only ops must stay at axon:read.
    assert_eq!(required_scope(&req_query()), Some("axon:read"));
}
#[test]
fn required_scope_uses_secure_defaults_and_promotes_llm_actions() {
    assert_eq!(
        required_scope(&AxonRequest::Ask(AskRequest {
            query: Some("q".into()),
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
