use super::*;
use axon_api::reset::{
    ResetCreated, ResetDeleted, ResetEstimate, ResetExecutionState, ResetPlan, ResetReceipt,
    ResetStorePlan,
};
use axon_core::config::Config;
use serial_test::serial;
use sqlx::Acquire;

fn cfg_with(stores: Vec<&str>, dry_run: bool, yes: bool) -> Config {
    let mut cfg = Config::test_default();
    cfg.reset_stores = stores.into_iter().map(str::to_string).collect();
    cfg.reset_dry_run = dry_run;
    cfg.yes = yes;
    cfg
}

#[test]
fn resolve_stores_defaults_to_all_in_canonical_order() {
    let cfg = cfg_with(vec![], false, false);
    let stores = resolve_stores(&cfg).expect("resolve");
    assert_eq!(
        stores,
        vec![
            "jobs".to_string(),
            "ledger".to_string(),
            "code_index".to_string(),
            "watch".to_string(),
            "graph".to_string(),
            "memory".to_string(),
            "vectors".to_string(),
            "artifacts".to_string(),
        ]
    );
}

#[test]
fn resolve_stores_dedups_and_canonicalizes_order() {
    // Passed out of order + duplicated — must come back canonical + unique.
    let cfg = cfg_with(vec!["vectors", "jobs", "jobs"], false, false);
    let stores = resolve_stores(&cfg).expect("resolve");
    assert_eq!(
        stores,
        vec![
            "jobs".to_string(),
            "ledger".to_string(),
            "code_index".to_string(),
            "watch".to_string(),
            "graph".to_string(),
            "memory".to_string(),
            "vectors".to_string(),
        ]
    );
}

#[test]
fn resolve_stores_rejects_unknown_store() {
    let cfg = cfg_with(vec!["bogus"], false, false);
    let err = resolve_stores(&cfg).expect_err("unknown store must error");
    assert!(err.to_string().contains("unknown reset store"));
    assert!(err.to_string().contains("bogus"));
}

#[test]
fn dry_run_is_default_without_yes() {
    // No --yes ⇒ dry-run even without an explicit --dry-run.
    assert!(is_dry_run(&cfg_with(vec![], false, false)));
}

#[test]
fn yes_disables_dry_run() {
    assert!(!is_dry_run(&cfg_with(vec![], false, true)));
}

#[test]
fn explicit_dry_run_pins_dry_run_even_with_yes() {
    // --dry-run wins even when --yes is set: no destruction.
    assert!(is_dry_run(&cfg_with(vec![], true, true)));
}

#[test]
fn wants_any_sqlite_true_for_ledger_only() {
    let stores = vec!["ledger".to_string()];
    assert!(wants_any_sqlite(&stores));
}

#[test]
fn wants_any_sqlite_false_for_vectors_and_artifacts_only() {
    let stores = vec!["vectors".to_string(), "artifacts".to_string()];
    assert!(!wants_any_sqlite(&stores));
}

#[tokio::test]
async fn dry_run_reset_mutates_nothing_and_reports_plan() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("jobs.db");
    let mut cfg = cfg_with(vec!["jobs"], false, false);
    cfg.sqlite_path = db_path.clone();

    let result = reset(&cfg).await.expect("dry-run reset");
    assert!(result.dry_run, "default reset must be a dry-run");
    assert!(
        result.receipt_path.is_some(),
        "dry-run reports the receipt path execution would write"
    );
    // DB was never created by a read-only inventory.
    assert!(!db_path.exists(), "dry-run must not create the DB");
    assert!(result.deleted.sqlite_tables == 0);
    assert_eq!(result.stores.len(), SQLITE_STORES.len());
    assert_eq!(result.plan.len(), SQLITE_STORES.len());
    assert_eq!(result.plan[0].store, "jobs");
}

#[tokio::test]
async fn sqlite_wipe_and_remigrate_yields_fresh_schema() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("jobs.db");
    // A fresh wipe on a non-existent DB re-migrates cleanly.
    let schema = sqlite::wipe_and_remigrate(&db_path)
        .await
        .expect("wipe + remigrate");
    assert!(
        schema.version > 0,
        "re-migrated DB should have applied migrations"
    );
    assert_eq!(schema.checksum.len(), 64);
    assert!(db_path.exists());

    // Inventory of the fresh DB: tables present, zero content rows.
    let inv = sqlite::inventory(&db_path).await.expect("inventory");
    assert!(inv.exists);
    assert!(inv.table_count > 0);
    assert_eq!(inv.content_rows, 0);
    assert!(!inv.non_empty());
    assert_eq!(inv.schema_identity, schema);
    for table in [
        "axon_applied_migrations",
        "axon_observe_events",
        "axon_observe_heartbeats",
        "axon_observe_provider_health",
    ] {
        assert!(
            inv.tables.contains_key(table),
            "composed inventory must include {table}"
        );
    }

    let pool = axon_core::sqlite::open_pool_unlocked(&db_path.to_string_lossy())
        .await
        .expect("open migrated pool");
    for table in [
        "axon_crawl_jobs",
        "axon_embed_jobs",
        "axon_extract_jobs",
        "axon_ingest_jobs",
        "axon_ingest_payloads",
        "axon_job_cutover_receipts",
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|e| panic!("probe {table}: {e}"));
        assert_eq!(count, 0, "removed table {table} must not exist");
    }
    pool.close().await;
}

#[tokio::test]
async fn reset_writes_no_unified_job_row_for_itself() {
    // `reset` is intentionally NOT job-tracked (see docs/pipeline-unification/
    // plans/2026-07-04-full-durable-job-cutover.md, "Scope Exception: `reset`
    // Stays Jobless"): its dry-run default must not create/migrate the SQLite
    // DB at all, and any job-backed tracking path
    // (enqueue_operation/start_operation_job/complete_operation_job) does
    // exactly that as a side effect of writing a job row. Prove a real
    // (--yes) reset that wipes the `jobs` store leaves the freshly
    // re-migrated unified `jobs` table empty -- reset never enqueues a job
    // for its own execution.
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("jobs.db");

    let mut cfg = cfg_with(vec!["jobs"], false, false);
    cfg.sqlite_path = db_path.clone();
    let plan = reset(&cfg).await.expect("reset plan");
    cfg.yes = true;
    cfg.reset_plan_id = Some(plan.plan_id);
    let result = reset_with_authz(&cfg, &ResetAuthz::admin())
        .await
        .expect("real reset");
    assert!(!result.dry_run);

    let pool = axon_core::sqlite::open_pool_unlocked(&db_path.to_string_lossy())
        .await
        .expect("open post-reset pool");
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs")
        .fetch_one(&pool)
        .await
        .expect("count jobs rows");
    pool.close().await;

    assert_eq!(
        count.0, 0,
        "reset must not create a unified job row for its own execution"
    );
}

#[tokio::test]
async fn destructive_reset_requires_reviewed_plan_admin_and_confirmation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cfg = cfg_with(vec!["jobs"], false, true);
    cfg.sqlite_path = dir.path().join("jobs.db");

    let missing = reset_with_authz(&cfg, &ResetAuthz::admin())
        .await
        .expect_err("plan id is required");
    assert!(missing.to_string().contains("reset.plan_required"));

    cfg.yes = false;
    let plan = reset(&cfg).await.expect("plan");
    cfg.yes = true;
    cfg.reset_plan_id = Some(plan.plan_id.clone());
    let denied = reset_with_authz(&cfg, &ResetAuthz::anonymous())
        .await
        .expect_err("admin is required");
    assert!(denied.to_string().contains("reset.admin_required"));

    let result = reset_with_authz(&cfg, &ResetAuthz::admin())
        .await
        .expect("reviewed plan executes");
    assert_eq!(result.plan_id, plan.plan_id);
    assert!(
        result
            .audit_events
            .iter()
            .any(|event| event == "reset.confirm")
    );
    assert!(
        result
            .chunks
            .iter()
            .all(|chunk| chunk.status == "completed")
    );
    let repeated = reset_with_authz(&cfg, &ResetAuthz::admin())
        .await
        .expect("completed plan id is reusable and idempotent");
    assert_eq!(repeated.deleted, result.deleted);
    assert_eq!(repeated.receipt_path, result.receipt_path);
}

#[tokio::test]
#[serial]
async fn reset_receipt_redacts_secrets_before_writing() {
    // This crate denies `unsafe`, which `std::env::set_var` now requires, so
    // this writes under the real `axon_data_base_dir()` (matching the
    // `reddit_acquire.rs`/`youtube_acquire_tests.rs` precedent for the same
    // constraint) with a uniquely-named reset id, then cleans up after.
    let reset_id = "phase3b-redaction-test";
    let plan_id = "phase3b-redaction-test-plan";
    let reset_plan = ResetPlan {
        plan_id: plan_id.to_string(),
        reset_id: reset_id.to_string(),
        stores: Vec::new(),
        estimates: ResetEstimate::default(),
        inventory_checksum: String::new(),
        config_snapshot_id: String::new(),
        auth_snapshot_id: String::new(),
        confirmation_text: String::new(),
        receipt_path: None,
        expires_at_utc: String::new(),
        blockers: Vec::new(),
    };
    let receipt = ResetReceipt {
        plan_id: plan_id.to_string(),
        reset_id: reset_id.to_string(),
        state: ResetExecutionState::Completed,
        chunks: Vec::new(),
        deleted: ResetDeleted::default(),
        created: ResetCreated::default(),
        audit_events: Vec::new(),
    };
    let path = write_receipt(
        reset_id,
        &[],
        &[] as &[ResetStorePlan],
        &reset_plan,
        &receipt,
        &["provider error: Authorization: Bearer abcdef0123456789abcdef".to_string()],
    )
    .await
    .expect("write receipt");

    let written = std::fs::read_to_string(&path).expect("read receipt");
    let _ = std::fs::remove_file(&path);
    assert!(!written.contains("abcdef0123456789abcdef"));
}

#[tokio::test]
async fn reset_resume_validates_completed_sqlite_postcondition_per_chunk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cfg = cfg_with(vec!["jobs"], false, false);
    cfg.sqlite_path = dir.path().join("jobs.db");
    let saved = reset(&cfg).await.expect("reviewed plan");

    let schema = sqlite::wipe_and_remigrate(&cfg.sqlite_path)
        .await
        .expect("simulate completed sqlite chunk");
    let current = prepare_reset(
        &cfg,
        saved.stores.clone(),
        saved.reset_id.clone(),
        saved.plan_id.clone(),
        false,
    )
    .await
    .expect("current inventory");
    let mut chunks = execution::planned_chunks(&saved.stores);
    chunks[0].status = "completed".to_string();
    chunks[0].checkpoint = format!(
        "schema_version={};schema_checksum={}",
        schema.version, schema.checksum
    );
    let receipt = ResetReceipt {
        plan_id: saved.plan_id.clone(),
        reset_id: saved.reset_id.clone(),
        state: ResetExecutionState::Executing,
        chunks,
        deleted: ResetDeleted::default(),
        created: ResetCreated::default(),
        audit_events: vec!["reset.chunk.complete:sqlite".to_string()],
    };

    recovery::validate_resumable_inventory(&saved, &current, Some(&receipt))
        .expect("completed chunk validates its postcondition, not the original checksum");

    let mut failed_receipt = receipt.clone();
    failed_receipt.chunks[0].status = "failed".to_string();
    recovery::validate_resumable_inventory(&saved, &current, Some(&failed_receipt))
        .expect("failed chunk may resume a bounded remainder of the reviewed store");

    let pool = axon_core::sqlite::open_pool_unlocked(&cfg.sqlite_path.to_string_lossy())
        .await
        .expect("open reset DB");
    sqlx::query("CREATE TABLE unexpected_after_reset (id INTEGER PRIMARY KEY)")
        .execute(&pool)
        .await
        .expect("mutate completed schema");
    pool.close().await;
    let changed = prepare_reset(
        &cfg,
        saved.stores.clone(),
        saved.reset_id.clone(),
        saved.plan_id.clone(),
        false,
    )
    .await
    .expect("changed inventory");
    let error = recovery::validate_resumable_inventory(&saved, &changed, Some(&receipt))
        .expect_err("completed postcondition drift must be rejected");
    assert!(error.to_string().contains("completed_chunk_changed"));
}

#[tokio::test]
async fn expired_reset_plan_rejects_first_start_but_allows_started_resume() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cfg = cfg_with(vec!["jobs"], false, false);
    cfg.sqlite_path = dir.path().join("jobs.db");
    let mut saved = reset(&cfg).await.expect("reviewed plan");
    saved.plan_expires_at_utc = (chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
    saved.reset_plan.expires_at_utc = saved.plan_expires_at_utc.clone();
    plan_store::save_plan(&cfg, &saved)
        .await
        .expect("persist expired plan");

    cfg.yes = true;
    cfg.reset_plan_id = Some(saved.plan_id.clone());
    let error = reset_with_authz(&cfg, &ResetAuthz::admin())
        .await
        .expect_err("expired unstarted plan must be rejected");
    assert!(error.to_string().contains("plan_expired"));

    let started = ResetReceipt {
        plan_id: saved.plan_id.clone(),
        reset_id: saved.reset_id.clone(),
        state: ResetExecutionState::Executing,
        chunks: execution::planned_chunks(&saved.stores),
        deleted: ResetDeleted::default(),
        created: ResetCreated::default(),
        audit_events: vec!["reset.execute".to_string()],
    };
    plan_store::save_receipt(&cfg, &started)
        .await
        .expect("persist started checkpoint");

    let resumed = reset_with_authz(&cfg, &ResetAuthz::admin())
        .await
        .expect("started plan remains resumable after expiry");
    assert_eq!(resumed.execution_state, ResetExecutionState::Completed);
    assert!(
        resumed
            .audit_events
            .iter()
            .any(|event| event == "reset.resume")
    );
}

#[tokio::test]
async fn sqlite_reset_refuses_a_concurrent_writer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("jobs.db");
    sqlite::wipe_and_remigrate(&db_path)
        .await
        .expect("seed canonical DB");
    let blocker = axon_core::sqlite::open_pool_unlocked(&db_path.to_string_lossy())
        .await
        .expect("open competing writer");
    let mut transaction = blocker.begin().await.expect("begin competing transaction");
    sqlx::query("CREATE TABLE writer_holds_reset_lock (id INTEGER PRIMARY KEY)")
        .execute(&mut *transaction)
        .await
        .expect("hold write lock");

    let error = sqlite::wipe_and_remigrate(&db_path)
        .await
        .expect_err("reset must fail closed while another writer holds the DB");

    assert!(
        error.to_string().contains("locked") || error.to_string().contains("sqlite_not_exclusive")
    );
    transaction
        .rollback()
        .await
        .expect("release competing writer");
    blocker.close().await;
}
