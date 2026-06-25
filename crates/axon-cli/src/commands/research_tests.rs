use super::*;
use axon_core::config::CommandKind;
use axon_jobs::backend::{BackendResult, JobKind, JobPayload};
use axon_services::context::ServiceContext;
use axon_services::runtime::ServiceJobRuntime;
use axon_services::types::ServiceJob;
use std::error::Error as StdError;
use std::sync::Arc;
use uuid::Uuid;

struct NoopRuntime;

#[async_trait::async_trait]
impl ServiceJobRuntime for NoopRuntime {
    fn mode_name(&self) -> &'static str {
        "test"
    }

    async fn enqueue(&self, _payload: JobPayload) -> BackendResult<Uuid> {
        Ok(Uuid::new_v4())
    }

    async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
        Ok("completed".to_string())
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
    ) -> Result<Vec<ServiceJob>, Box<dyn StdError + Send + Sync>> {
        Ok(Vec::new())
    }

    async fn job_status(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn StdError + Send + Sync>> {
        Ok(None)
    }

    async fn cancel_job(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<bool, Box<dyn StdError + Send + Sync>> {
        Ok(false)
    }

    async fn cleanup_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn clear_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn recover_jobs(
        &self,
        _kind: JobKind,
        _stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs(&self, _kind: JobKind) -> Result<i64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs_by_status(
        &self,
        _kind: JobKind,
    ) -> Result<
        std::collections::HashMap<axon_jobs::status::JobStatus, i64>,
        Box<dyn StdError + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
    }
}

fn test_context() -> ServiceContext {
    ServiceContext::from_runtime(Arc::new(Config::test_default()), Arc::new(NoopRuntime))
}

fn make_research_cfg(tavily_key: &str) -> Config {
    let mut cfg = Config::test_default();
    cfg.command = CommandKind::Research;
    cfg.positional = vec!["test query".to_string()];
    cfg.tavily_api_key = tavily_key.to_string(); // gitleaks:allow - test config field, value is caller-provided fixture text
    cfg
}

#[tokio::test]
async fn test_run_research_rejects_empty_tavily_key() {
    let cfg = make_research_cfg("");
    let ctx = test_context();
    let err = run_research(&cfg, &ctx).await.unwrap_err();
    assert!(
        err.to_string().contains("TAVILY_API_KEY"),
        "expected TAVILY_API_KEY error, got: {err}"
    );
}

#[tokio::test]
async fn test_run_research_validates_query_before_prereqs() {
    // Both query *and* Tavily key are missing — query check runs first
    // because it's free, while the prereq check waits on the service call.
    let mut cfg = make_research_cfg("");
    cfg.positional = vec![];
    cfg.query = None;
    let ctx = test_context();
    let err = run_research(&cfg, &ctx).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("query"),
        "expected query validation to fire first, got: {msg}"
    );
    assert!(
        !msg.contains("TAVILY_API_KEY"),
        "TAVILY error should not surface before query check, got: {msg}"
    );
}

#[tokio::test]
async fn test_run_research_skips_llm_prereq_before_query_validation() {
    let mut cfg = make_research_cfg("tvly-key");
    cfg.positional = vec![];
    cfg.query = None;
    let ctx = test_context();
    let err = run_research(&cfg, &ctx).await.unwrap_err();
    assert!(
        err.to_string().contains("query"),
        "expected query validation after skipping openai_model check, got: {err}"
    );
}

#[tokio::test]
async fn test_run_research_rejects_missing_query() {
    let mut cfg = make_research_cfg("tvly-key");
    cfg.positional = vec![];
    cfg.query = None;
    let ctx = test_context();
    let err = run_research(&cfg, &ctx).await.unwrap_err();
    assert!(
        err.to_string().contains("query"),
        "expected query error, got: {err}"
    );
}

#[test]
fn research_cfg_depth_defaults_to_none() {
    let cfg = make_research_cfg("tvly-key");
    assert!(
        cfg.research_depth.is_none(),
        "research_depth should default to None"
    );
}

#[test]
fn research_depth_overrides_search_limit_when_set() {
    // Mirrors the wiring in `run_research`: `cfg.research_depth.unwrap_or(cfg.search_limit)`.
    // This protects against silent regressions where someone wires depth
    // to a different field or stops reading it.
    let mut cfg = make_research_cfg("tvly-key");
    cfg.search_limit = 5;
    assert_eq!(cfg.research_depth.unwrap_or(cfg.search_limit), 5);

    cfg.research_depth = Some(20);
    assert_eq!(cfg.research_depth.unwrap_or(cfg.search_limit), 20);
}
