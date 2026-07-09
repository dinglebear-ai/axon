//! Phase-3b Task 11 end-to-end failure guard: a job's auth snapshot is
//! recorded once at creation and must survive stale reclaim unchanged — a
//! reclaimed/retried job must never gain scope it wasn't originally granted.
//!
//! `auth_snapshot_json` has exactly one writer (job creation) and one reader
//! (the unified claim query) in the whole crate — recovery/reclaim only ever
//! touches status/attempt columns — so this property holds by construction.
//! This test proves it end-to-end through the real claim → stale → recover →
//! reclaim → run pipeline, rather than just asserting the enforcement helper
//! in isolation (see `workers::auth_enforcement_tests`).

use axon_api::source::*;
use tokio_util::sync::CancellationToken;

use super::unified;
use crate::boundary::JobStore;
use crate::store::open_sqlite_pool;
use crate::unified::SqliteUnifiedJobStore;

async fn seed_source(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "INSERT OR IGNORE INTO sources (
            source_id, committed_generation, summary_json, created_at, updated_at
        ) VALUES ('src_e2e', NULL, '{}', '1970-01-01T00:00:00Z', '1970-01-01T00:00:00Z')",
    )
    .execute(pool)
    .await
    .expect("seed source row");
}

fn reset_job_request() -> JobCreateRequest {
    JobCreateRequest {
        request_id: Some("req_e2e_reset".to_string()),
        job_kind: JobKind::Reset,
        job_intent: JobIntent::Run,
        source_id: Some(SourceId::new("src_e2e")),
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority: JobPriority::Normal,
        idempotency_key: Some("idem-e2e-reset".to_string()),
        stage_plan: vec![JobStagePlan {
            phase: PipelinePhase::Planning,
            required: true,
            provider_requirements: Vec::new(),
            estimated_items: Some(1),
        }],
        request: Some(serde_json::json!({"operation": "reset"})),
        // Default snapshot grants nothing — a Reset job requires
        // `axon:admin` (see `auth_enforcement::required_scope_for_kind`).
        auth_snapshot: AuthSnapshot::default(),
        config_snapshot_id: Some(ConfigSnapshotId::new("cfg_e2e")),
        requirements: MetadataMap::new(),
        result_schema: Some("job_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn recovered_job_uses_original_auth_snapshot() {
    let pool = open_sqlite_pool(":memory:").await.expect("open sqlite");
    seed_source(&pool).await;
    let store = SqliteUnifiedJobStore::new(pool.clone());

    // Create + claim a Reset job with a snapshot that was never granted
    // `axon:admin`.
    let job = store
        .create(reset_job_request())
        .await
        .expect("create reset job");
    let first_claim = unified::claim_next_unified_job(&pool)
        .await
        .expect("claim query")
        .expect("job should be claimable");
    assert_eq!(first_claim.job_id, job.job_id);
    assert!(
        !first_claim
            .auth_snapshot
            .granted_scopes
            .contains(&AuthScope::Admin),
        "seeded snapshot must not carry admin scope"
    );

    // Force it stale (an interrupted worker) and reclaim it — `stale_before`
    // set to a future instant marks the just-claimed running job stale
    // deterministically, without sleeping in the test.
    let stale_before = Timestamp::from(chrono::Utc::now() + chrono::Duration::seconds(3600));
    let recovery = store
        .recover(JobRecoveryRequest {
            kind: Some(JobKind::Reset),
            stale_before: Some(stale_before),
            limit: None,
            older_than_seconds: None,
            dry_run: false,
            allow_without_cutoff: false,
        })
        .await
        .expect("recover");
    assert_eq!(recovery.jobs_scanned, 1);
    assert_eq!(recovery.jobs_requeued, 1);

    // Re-claim the reclaimed job and run it through the real unified runner —
    // it must still be blocked on the *original* snapshot, not upgraded by
    // going through recovery.
    let reclaimed_claim = unified::claim_next_unified_job(&pool)
        .await
        .expect("claim query")
        .expect("reclaimed job should be claimable again");
    assert_eq!(reclaimed_claim.job_id, job.job_id);
    assert_eq!(
        reclaimed_claim.auth_snapshot, first_claim.auth_snapshot,
        "reclaim must not alter the recorded auth snapshot"
    );

    unified::run_unified_claimed(&pool, &reclaimed_claim, &CancellationToken::new(), None).await;

    let summary = store.get(job.job_id).await.unwrap().unwrap();
    assert_eq!(summary.status, LifecycleStatus::Failed);
    let last_error = summary.last_error.expect("failure must record an error");
    assert_eq!(last_error.code.to_string(), "auth.scope_required");
}
