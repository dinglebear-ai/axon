use crate::context::ServiceContext;
use async_trait::async_trait;
use axon_core::config::{CommandKind, Config, FreshnessCommand, FreshnessRequest};
use axon_jobs::backend::{BackendResult, JobKind, JobPayload, JobSidecarPayload};
use axon_jobs::freshness::FreshnessDef;
use axon_jobs::status::JobStatus;
use serde_json::json;
use std::error::Error as StdError;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::freshness::{
    FreshnessRequestPayload, FreshnessRequestV1, create_from_config, dispatch_freshness,
    freshness_identity_hash, run_fake_freshness_scheduler_with_limits, run_now,
    safe_replay_snapshot, validate_freshness_payload_for_dispatch,
};

#[test]
fn safe_replay_snapshot_does_not_persist_secret_headers() {
    let mut cfg = Config::test_default();
    cfg.custom_headers = vec![
        "Authorization: Bearer sk-secret".to_string(),
        "Cookie: sid=secret".to_string(),
        "X-Docs-Version: latest".to_string(),
    ];
    let err = safe_replay_snapshot(&cfg).unwrap_err();
    assert!(
        err.to_string()
            .contains("secret-bearing headers cannot be stored in freshness schedules")
    );
}

#[test]
fn safe_replay_snapshot_strips_freshness_intent() {
    let mut cfg = Config::test_default();
    cfg.freshness = Some(FreshnessRequest {
        command: FreshnessCommand::Scrape,
        every_seconds: 86_400,
    });
    let snapshot = safe_replay_snapshot(&cfg).unwrap();
    assert!(snapshot.freshness_is_stripped);
}

#[test]
fn identity_hash_distinguishes_collection_and_render_mode() {
    let a = freshness_identity_hash(
        "scrape",
        "https://example.com",
        86_400,
        &json!({"url":"https://example.com"}),
        &json!({"collection":"a","render_mode":"http"}),
    );
    let b = freshness_identity_hash(
        "scrape",
        "https://example.com",
        86_400,
        &json!({"url":"https://example.com"}),
        &json!({"collection":"b","render_mode":"http"}),
    );
    assert_ne!(a, b);
}

#[cfg(unix)]
#[test]
fn dispatch_validation_rejects_local_embed_replaced_by_symlink_escape() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let allowed = tmp.path().join("allowed");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&allowed).expect("allowed dir");
    std::fs::create_dir_all(&outside).expect("outside dir");
    let input = allowed.join("doc.md");
    std::fs::write(&input, "# safe").expect("safe file");

    let mut cfg = Config::test_default();
    cfg.mcp_embed_allowed_roots = vec![allowed.clone()];
    let payload = FreshnessRequestPayload::V1(FreshnessRequestV1::Embed {
        input: input.to_string_lossy().to_string(),
    });
    validate_freshness_payload_for_dispatch(&payload, &cfg).expect("initial file is valid");

    std::fs::remove_file(&input).expect("remove file");
    let secret = outside.join("secret.md");
    std::fs::write(&secret, "# secret").expect("outside file");
    std::os::unix::fs::symlink(&secret, &input).expect("symlink");

    let err = validate_freshness_payload_for_dispatch(&payload, &cfg)
        .expect_err("symlink escape must fail at dispatch time");
    assert!(
        err.to_string().contains("must be under one of")
            || err.to_string().contains("must not be a symlink"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn scheduler_limits_concurrent_dispatches() {
    let max_seen = run_fake_freshness_scheduler_with_limits(20, 2).await;
    assert_eq!(max_seen, 2);
}

#[tokio::test]
async fn wait_true_uses_manual_lease_and_does_not_double_fire() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::test_default();
    cfg.sqlite_path = temp.path().join("jobs.db");
    cfg.command = CommandKind::Embed;
    cfg.positional = vec!["fresh text".to_string()];
    cfg.wait = true;
    cfg.freshness = Some(FreshnessRequest {
        command: FreshnessCommand::Embed,
        every_seconds: 86_400,
    });

    let ctx = ServiceContext::new(Arc::new(cfg.clone()))
        .await
        .expect("service context");
    let schedule = create_from_config(&cfg, &ctx).await.expect("schedule");
    assert!(
        schedule.next_run_at.timestamp_millis() > axon_jobs::store::now_ms(),
        "created schedules should start in the future"
    );

    let run = run_now(&ctx, schedule.id).await.expect("manual run");
    assert_eq!(run.status, "enqueued");
    let history = crate::freshness::history(&ctx, schedule.id, 10)
        .await
        .expect("history");
    assert_eq!(history.len(), 1);
}

#[tokio::test]
async fn stored_freshness_intent_is_not_replayed_recursively() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::test_default();
    cfg.sqlite_path = temp.path().join("jobs.db");
    cfg.command = CommandKind::Embed;
    cfg.positional = vec!["fresh text".to_string()];
    cfg.freshness = Some(FreshnessRequest {
        command: FreshnessCommand::Embed,
        every_seconds: 86_400,
    });

    let ctx = ServiceContext::new(Arc::new(cfg.clone()))
        .await
        .expect("service context");
    let schedule = create_from_config(&cfg, &ctx).await.expect("schedule");
    let before = crate::freshness::list(&ctx, 10).await.expect("before");
    assert_eq!(before.len(), 1);

    run_now(&ctx, schedule.id).await.expect("manual run");
    let after = crate::freshness::list(&ctx, 10).await.expect("after");
    assert_eq!(after.len(), 1, "replay must not create nested schedules");
}

#[tokio::test]
async fn active_embed_job_is_recorded_as_skipped_without_duplicate_enqueue() {
    let mut cfg = Config::test_default();
    cfg.command = CommandKind::Embed;
    let runtime = Arc::new(ActiveRuntime::with_active_target("fresh text"));
    let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), runtime.clone());
    let replay = safe_replay_snapshot(&cfg).expect("snapshot");
    let payload = FreshnessRequestPayload::V1(FreshnessRequestV1::Embed {
        input: "fresh text".to_string(),
    });
    let def = FreshnessDef {
        id: Uuid::new_v4(),
        name: "embed:fresh text".to_string(),
        command: "embed".to_string(),
        target: "fresh text".to_string(),
        identity_hash: "0123456789abcdef".to_string(),
        request_json: serde_json::to_value(payload).expect("payload json"),
        config_json: serde_json::to_value(replay).expect("config json"),
        every_seconds: 86_400,
        enabled: true,
        next_run_at: chrono::Utc::now(),
        lease_expires_at: None,
        last_run_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let outcome = dispatch_freshness(&ctx, &def).await.expect("dispatch");
    assert_eq!(outcome.status, "skipped_active_job");
    assert_eq!(runtime.enqueued_count(), 0);
}

struct ActiveRuntime {
    active_target: Option<String>,
    payloads: Mutex<Vec<JobPayload>>,
}

impl ActiveRuntime {
    fn with_active_target(target: &str) -> Self {
        Self {
            active_target: Some(target.to_string()),
            payloads: Mutex::new(Vec::new()),
        }
    }

    fn enqueued_count(&self) -> usize {
        self.payloads.lock().expect("payloads").len()
    }
}

#[async_trait]
impl crate::runtime::ServiceJobRuntime for ActiveRuntime {
    fn mode_name(&self) -> &'static str {
        "test"
    }

    async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid> {
        self.payloads.lock().expect("payloads").push(payload);
        Ok(Uuid::new_v4())
    }

    async fn enqueue_with_sidecar(
        &self,
        payload: JobPayload,
        _sidecar: JobSidecarPayload,
    ) -> BackendResult<Uuid> {
        self.enqueue(payload).await
    }

    async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
        Ok("completed".to_string())
    }

    async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
        Ok(None)
    }

    async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
        Ok(self.active_target.is_some())
    }

    async fn list_jobs(
        &self,
        _kind: JobKind,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<crate::types::ServiceJob>, Box<dyn StdError + Send + Sync>> {
        Ok(self
            .active_target
            .as_ref()
            .map(|target| crate::types::ServiceJob {
                id: Uuid::new_v4(),
                status: "pending".to_string(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                started_at: None,
                finished_at: None,
                error_text: None,
                url: None,
                source_type: None,
                target: Some(target.clone()),
                urls_json: None,
                progress_json: None,
                result_json: None,
                config_json: None,
                attempt_count: 0,
                active_attempt_id: None,
                last_reclaimed_at: None,
                last_reclaimed_reason: None,
            })
            .into_iter()
            .collect())
    }

    async fn job_status(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<Option<crate::types::ServiceJob>, Box<dyn StdError + Send + Sync>> {
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
    ) -> Result<std::collections::HashMap<JobStatus, i64>, Box<dyn StdError + Send + Sync>> {
        Ok(std::collections::HashMap::new())
    }
}
