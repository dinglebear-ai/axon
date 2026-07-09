use super::*;
use axon_api::reset::{
    ResetCreated, ResetDeleted, ResetEstimate, ResetExecutionState, ResetPlan, ResetReceipt,
    ResetStorePlan,
};
use axon_core::config::{Config, ConfigValueSource};
use serial_test::serial;

fn cfg_with(stores: Vec<&str>, dry_run: bool, yes: bool) -> Config {
    let mut cfg = Config::test_default();
    cfg.reset_stores = stores.into_iter().map(str::to_string).collect();
    cfg.reset_dry_run = dry_run;
    cfg.yes = yes;
    cfg
}

/// Seed a legacy `axon_crawl_jobs` row directly (via the migrated unified
/// SQLite DB), mirroring `real_reset_records_legacy_cutover_receipt`'s own
/// seeding approach. Returns the config pointed at the seeded DB.
async fn test_config_with_legacy_crawl_job_row() -> Config {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("jobs.db");
    let version = sqlite::wipe_and_remigrate(&db_path)
        .await
        .expect("initial migrate");
    assert!(version > 0);
    {
        let pool = axon_core::sqlite::open_pool_unlocked(&db_path.to_string_lossy())
            .await
            .expect("open pool");
        sqlx::query(
            "INSERT INTO axon_crawl_jobs (id, created_at, updated_at) VALUES ('legacy-1', 0, 0)",
        )
        .execute(&pool)
        .await
        .expect("seed legacy row");
        pool.close().await;
    }
    std::mem::forget(dir);
    let mut cfg = cfg_with(vec!["jobs"], false, false);
    cfg.sqlite_path = db_path;
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
    assert_eq!(stores, vec!["jobs".to_string(), "vectors".to_string()]);
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
    assert_eq!(result.stores, vec!["jobs".to_string()]);
    assert_eq!(result.plan.len(), 1);
    assert_eq!(result.plan[0].store, "jobs");
}

#[tokio::test]
async fn sqlite_wipe_and_remigrate_yields_fresh_schema() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("jobs.db");
    // A fresh wipe on a non-existent DB re-migrates cleanly.
    let version = sqlite::wipe_and_remigrate(&db_path)
        .await
        .expect("wipe + remigrate");
    assert!(version > 0, "re-migrated DB should have applied migrations");
    assert!(db_path.exists());

    // Inventory of the fresh DB: tables present, zero content rows.
    let inv = sqlite::inventory(&db_path).await.expect("inventory");
    assert!(inv.exists);
    assert!(inv.table_count > 0);
    assert_eq!(inv.content_rows, 0);
    assert!(!inv.non_empty());
}

#[tokio::test]
async fn real_reset_records_legacy_cutover_receipt() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("jobs.db");

    // Seed a legacy job row before the reset destroys everything.
    let version = sqlite::wipe_and_remigrate(&db_path)
        .await
        .expect("initial migrate");
    assert!(version > 0);
    {
        let pool = axon_core::sqlite::open_pool_unlocked(&db_path.to_string_lossy())
            .await
            .expect("open pool");
        sqlx::query(
            "INSERT INTO axon_crawl_jobs (id, created_at, updated_at) VALUES ('legacy-1', 0, 0)",
        )
        .execute(&pool)
        .await
        .expect("seed legacy row");
        pool.close().await;
    }

    let mut cfg = cfg_with(vec!["jobs"], false, true);
    cfg.sqlite_path = db_path.clone();
    // A non-empty legacy job table now requires distinct, CLI-flag-sourced
    // confirmation before `reset()` will mutate anything — see
    // `legacy_wipe_confirmation_from_cli_flag_wipes_and_records_receipt`
    // below for the confirmation-gate-specific assertions.
    cfg.reset_confirm_legacy_wipe = true;
    cfg.reset_confirm_legacy_wipe_source = ConfigValueSource::CliFlag;

    let result = reset(&cfg).await.expect("real reset");
    assert!(!result.dry_run);

    let pool = axon_core::sqlite::open_pool_unlocked(&db_path.to_string_lossy())
        .await
        .expect("open post-reset pool");
    let row: (String, String) = sqlx::query_as(
        "SELECT receipt_kind, message FROM axon_job_cutover_receipts ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("fetch receipt");
    pool.close().await;

    assert_eq!(row.0, "legacy_reset");
    assert!(row.1.contains("axon_crawl_jobs"));
}

#[tokio::test]
async fn dry_run_plan_surfaces_non_empty_legacy_job_tables_as_a_blocker() {
    let cfg = test_config_with_legacy_crawl_job_row().await;
    let result = reset(&cfg).await.expect("dry-run reset");
    assert!(result.dry_run);
    assert!(
        result
            .blockers
            .iter()
            .any(|b| b.contains("axon_crawl_jobs")),
        "expected a legacy-store blocker naming axon_crawl_jobs, got {:?}",
        result.blockers
    );
}

#[tokio::test]
async fn legacy_wipe_confirmation_sourced_from_config_file_is_rejected() {
    let mut cfg = test_config_with_legacy_crawl_job_row().await;
    cfg.yes = true;
    cfg.reset_confirm_legacy_wipe = true;
    cfg.reset_confirm_legacy_wipe_source = ConfigValueSource::TomlFile;
    let err = reset(&cfg)
        .await
        .expect_err("config-sourced confirmation must be rejected");
    assert!(
        err.to_string()
            .contains("--confirm-legacy-wipe must be passed as a CLI flag"),
        "config-sourced confirmation must be rejected, got: {err}"
    );
}

#[tokio::test]
async fn legacy_wipe_confirmation_missing_is_rejected_even_with_yes() {
    let mut cfg = test_config_with_legacy_crawl_job_row().await;
    cfg.yes = true;
    // reset_confirm_legacy_wipe left at its default `false`/`Unset`.
    let err = reset(&cfg)
        .await
        .expect_err("--yes alone must not be enough to wipe a non-empty legacy store");
    assert!(
        err.to_string()
            .contains("reset.legacy_store_confirmation_required"),
        "expected the legacy-store confirmation-required error, got: {err}"
    );
}

#[tokio::test]
async fn legacy_wipe_confirmation_from_cli_flag_wipes_and_records_receipt() {
    let mut cfg = test_config_with_legacy_crawl_job_row().await;
    cfg.yes = true;
    cfg.reset_confirm_legacy_wipe = true;
    cfg.reset_confirm_legacy_wipe_source = ConfigValueSource::CliFlag;
    let result = reset(&cfg).await.expect("cli-flag-confirmed reset");
    assert!(!result.dry_run);
    assert!(
        result.audit_events.iter().any(|e| e.contains("legacy")),
        "expected a legacy-wipe audit event, got {:?}",
        result.audit_events
    );
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

    let mut cfg = cfg_with(vec!["jobs"], false, true);
    cfg.sqlite_path = db_path.clone();

    let result = reset(&cfg).await.expect("real reset");
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
