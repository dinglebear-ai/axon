use super::*;
use crate::runtime::ServiceJobRuntime;
use crate::types::ServiceJob;
use async_trait::async_trait;
use axon_api::mcp_schema::{
    AskRequest, BrandRequest, CrawlRequest, CrawlSubaction, DedupeRequest, DiffRequest,
    ElicitDemoRequest, EndpointsRequest, EvaluateRequest, ExtractRequest, ExtractSubaction,
    MemoryRequest, MemorySubaction, MigrateRequest, QueryRequest, ResearchRequest,
    ScreenshotRequest, StatusRequest, SuggestRequest,
};
use axon_core::config::Config;
use axon_jobs::backend::{BackendResult, JobKind, JobPayload};
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
        std::collections::HashMap<axon_jobs::status::JobStatus, i64>,
        Box<dyn Error + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
    }
}

fn test_context() -> ServiceContext {
    ServiceContext::from_runtime(Arc::new(Config::default()), Arc::new(EmptyRuntime))
}

struct StatusRuntime {
    job: ServiceJob,
}

#[async_trait]
impl ServiceJobRuntime for StatusRuntime {
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
        Ok(vec![self.job.clone()])
    }

    async fn job_status(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(Some(self.job.clone()))
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
        Ok(1)
    }

    async fn count_jobs_by_status(
        &self,
        _kind: JobKind,
    ) -> Result<
        std::collections::HashMap<axon_jobs::status::JobStatus, i64>,
        Box<dyn Error + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
    }
}

fn service_job(
    status: &str,
    progress_json: Option<serde_json::Value>,
    result_json: Option<serde_json::Value>,
) -> ServiceJob {
    let now = chrono::Utc::now();
    ServiceJob {
        id: Uuid::new_v4(),
        status: status.to_string(),
        created_at: now,
        updated_at: now,
        started_at: Some(now),
        finished_at: None,
        error_text: None,
        url: None,
        source_type: None,
        target: None,
        urls_json: Some(serde_json::json!(["https://example.com"])),
        progress_json,
        result_json,
        config_json: None,
        attempt_count: 1,
        active_attempt_id: Some("attempt-1".to_string()),
        last_reclaimed_at: None,
        last_reclaimed_reason: None,
    }
}

fn test_context_with_job(job: ServiceJob) -> ServiceContext {
    ServiceContext::from_runtime(Arc::new(Config::default()), Arc::new(StatusRuntime { job }))
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
            max_sitemaps: None,
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

#[tokio::test]
async fn services_action_api_extract_status_hides_active_terminal_result() {
    let job = service_job(
        "running",
        Some(serde_json::json!({
            "lifecycle_progress": 0.42,
            "phase": "extracting",
            "pages_crawled": 7
        })),
        Some(serde_json::json!({
            "extract_result": {
                "title": "stale previous attempt"
            }
        })),
    );
    let job_id = job.id.to_string();

    let result = dispatch_action(
        &test_context_with_job(job),
        AxonRequest::Extract(ExtractRequest {
            subaction: Some(ExtractSubaction::Status),
            urls: None,
            prompt: None,
            max_pages: None,
            render_mode: None,
            embed: None,
            job_id: Some(job_id),
            limit: None,
            offset: None,
            response_mode: None,
        }),
    )
    .await
    .expect("extract status dispatch");

    assert_eq!(result["job"]["status"], "running");
    assert_eq!(result["job"]["metrics"]["pages_crawled"], 7);
    assert!(result.get("extract_result").is_none());
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
        ..AskRequest::default()
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

fn req_memory(subaction: MemorySubaction) -> AxonRequest {
    AxonRequest::Memory(MemoryRequest {
        subaction: Some(subaction),
        id: None,
        source_id: Some("source".into()),
        target_id: Some("target".into()),
        edge_type: None,
        memory_type: None,
        title: None,
        body: Some("Remembered fact".into()),
        query: Some("Remembered".into()),
        project: Some("axon".into()),
        repo: None,
        file: None,
        status: None,
        confidence: None,
        limit: None,
        depth: None,
        token_budget: None,
        response_mode: None,
        amount: None,
        pinned: None,
        reason: None,
        memory_ids: None,
        strategy: None,
        archive_sources: None,
        records: None,
        import_mode: None,
        dry_run: None,
        export_scope: None,
        include_archived: None,
        include_working: None,
    })
}

fn req_endpoints() -> AxonRequest {
    AxonRequest::Endpoints(EndpointsRequest {
        url: Some("https://example.com".into()),
        include_bundles: None,
        first_party_only: None,
        unique_only: None,
        max_scripts: None,
        max_scan_bytes: None,
        verify: None,
        capture_network: None,
        probe_rpc: None,
        probe_rpc_subdomains: None,
        response_mode: None,
    })
}

fn req_screenshot() -> AxonRequest {
    AxonRequest::Screenshot(ScreenshotRequest {
        url: Some("https://example.com".into()),
        full_page: None,
        viewport: None,
        output: None,
        response_mode: None,
    })
}

fn req_brand() -> AxonRequest {
    AxonRequest::Brand(BrandRequest {
        url: "https://example.com".into(),
        render_mode: None,
        response_mode: None,
    })
}

fn req_diff() -> AxonRequest {
    AxonRequest::Diff(DiffRequest {
        url_a: "https://example.com/a".into(),
        url_b: "https://example.com/b".into(),
        render_mode: None,
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
fn required_scope_active_network_actions_are_write() {
    for req in [req_endpoints(), req_screenshot(), req_brand(), req_diff()] {
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
fn required_scope_memory_subactions_are_read_write() {
    assert_eq!(
        required_scope(&req_memory(MemorySubaction::Remember)),
        Some("axon:write")
    );
    assert_eq!(
        required_scope(&req_memory(MemorySubaction::List)),
        Some("axon:read")
    );
    assert_eq!(
        required_scope(&req_memory(MemorySubaction::Search)),
        Some("axon:read")
    );
    assert_eq!(
        required_scope(&req_memory(MemorySubaction::Show)),
        Some("axon:read")
    );
    assert_eq!(
        required_scope(&req_memory(MemorySubaction::Link)),
        Some("axon:write")
    );
    assert_eq!(
        required_scope(&req_memory(MemorySubaction::Supersede)),
        Some("axon:write")
    );
    assert_eq!(
        required_scope(&req_memory(MemorySubaction::Context)),
        Some("axon:read")
    );
    assert_eq!(
        required_scope(&req_memory(MemorySubaction::Reinforce)),
        Some("axon:write")
    );
    assert_eq!(
        required_scope(&req_memory(MemorySubaction::Contradict)),
        Some("axon:write")
    );
    assert_eq!(
        required_scope(&req_memory(MemorySubaction::Pin)),
        Some("axon:write")
    );
    assert_eq!(
        required_scope(&req_memory(MemorySubaction::Archive)),
        Some("axon:write")
    );
    assert_eq!(
        required_scope(&req_memory(MemorySubaction::Forget)),
        Some("axon:write")
    );
    assert_eq!(
        required_scope(&req_memory(MemorySubaction::Review)),
        Some("axon:read")
    );
    assert_eq!(
        required_scope(&req_memory(MemorySubaction::Compact)),
        Some("axon:write")
    );
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
            ..AskRequest::default()
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
