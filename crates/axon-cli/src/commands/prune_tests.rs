use super::*;
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

fn cfg_with_target(target: &str, generation: Option<&str>) -> Config {
    let mut cfg = Config::test_default();
    cfg.prune_target = Some(target.to_string());
    cfg.prune_generation = generation.map(str::to_string);
    cfg
}

// ---------------------------------------------------------------------
// `build_selector` — pure target/generation parsing, no I/O.
// ---------------------------------------------------------------------

#[test]
fn build_selector_bare_target_is_source_selector() {
    let cfg = cfg_with_target("owner/repo", None);
    let selector = build_selector(&cfg).expect("valid source selector");
    match selector {
        PruneSelector::Source { source_id } => {
            assert_eq!(source_id, SourceId::new("owner/repo"));
        }
        other => panic!("expected Source selector, got {other:?}"),
    }
}

#[test]
fn build_selector_with_generation_is_generation_selector() {
    let cfg = cfg_with_target("owner/repo", Some("gen-2"));
    let selector = build_selector(&cfg).expect("valid generation selector");
    match selector {
        PruneSelector::Generation {
            source_id,
            generation,
        } => {
            assert_eq!(source_id, SourceId::new("owner/repo"));
            assert_eq!(generation, SourceGenerationId::new("gen-2"));
        }
        other => panic!("expected Generation selector, got {other:?}"),
    }
}

#[test]
fn build_selector_collection_prefix_is_collection_selector() {
    let cfg = cfg_with_target("collection:axon-test", None);
    let selector = build_selector(&cfg).expect("valid collection selector");
    match selector {
        PruneSelector::Collection { collection } => {
            assert_eq!(collection, "axon-test");
        }
        other => panic!("expected Collection selector, got {other:?}"),
    }
}

#[test]
fn build_selector_collection_prefix_rejects_generation() {
    let cfg = cfg_with_target("collection:axon-test", Some("gen-1"));
    let err = build_selector(&cfg).expect_err("collection + generation must be rejected");
    assert!(err.to_string().contains("--generation"));
}

#[test]
fn build_selector_collection_prefix_rejects_empty_name() {
    let cfg = cfg_with_target("collection:", None);
    let err = build_selector(&cfg).expect_err("empty collection name must be rejected");
    assert!(err.to_string().contains("collection"));
}

#[test]
fn build_selector_missing_target_is_rejected() {
    let mut cfg = Config::test_default();
    cfg.prune_target = None;
    let err = build_selector(&cfg).expect_err("missing target must be rejected");
    assert!(err.to_string().contains("target"));
}

#[test]
fn build_selector_blank_target_is_rejected() {
    let cfg = cfg_with_target("   ", None);
    let err = build_selector(&cfg).expect_err("blank target must be rejected");
    assert!(err.to_string().contains("target"));
}

// ---------------------------------------------------------------------
// `run_prune` — subaction routing + the --confirm gate, before any service
// call (so these never need a live Qdrant).
// ---------------------------------------------------------------------

fn test_context(cfg: Arc<Config>) -> ServiceContext {
    ServiceContext::from_runtime(cfg, Arc::new(NoopRuntime))
}

#[tokio::test]
async fn run_prune_exec_without_confirm_is_rejected_before_any_service_call() {
    let mut cfg = cfg_with_target("owner/repo", None);
    cfg.positional = vec!["exec".to_string()];
    cfg.prune_confirm = false;

    let cfg_arc = Arc::new(cfg);
    let ctx = test_context(cfg_arc.clone());

    let err = run_prune(&cfg_arc, &ctx)
        .await
        .expect_err("exec without --confirm must be rejected");
    assert!(err.to_string().contains("--confirm"));
}

#[tokio::test]
async fn run_prune_rejects_unknown_subaction() {
    let mut cfg = cfg_with_target("owner/repo", None);
    cfg.positional = vec!["bogus".to_string()];

    let cfg_arc = Arc::new(cfg);
    let ctx = test_context(cfg_arc.clone());

    let err = run_prune(&cfg_arc, &ctx)
        .await
        .expect_err("unknown subaction must be rejected");
    assert!(err.to_string().contains("plan|exec"));
}

#[tokio::test]
async fn run_prune_requires_a_subcommand() {
    let mut cfg = cfg_with_target("owner/repo", None);
    cfg.positional = Vec::new();

    let cfg_arc = Arc::new(cfg);
    let ctx = test_context(cfg_arc.clone());

    let err = run_prune(&cfg_arc, &ctx)
        .await
        .expect_err("missing subcommand must be rejected");
    assert!(err.to_string().contains("subcommand"));
}
