use super::*;
use axon_core::config::Config;

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
    assert!(result.receipt_path.is_none(), "dry-run writes no receipt");
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
