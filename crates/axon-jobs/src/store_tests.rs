use super::*;
use crate::backend::JobKind;
use axon_core::sqlite::rollback_on_release;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;
use std::process::Command;
use uuid::Uuid;

#[test]
fn active_db_lock_registry_is_idempotent_per_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("jobs.db");
    std::fs::write(&db_path, b"not actually sqlite").expect("write db");

    reset_sqlite_runtime_health_for_tests();
    let before = active_db_lock_count_for_tests();
    hold_active_db_lock(&db_path).expect("first active lock");
    hold_active_db_lock(&db_path).expect("second active lock");

    assert_eq!(
        active_db_lock_count_for_tests(),
        before + 1,
        "repeated opens of the same SQLite path should not leak lock handles"
    );
}

#[tokio::test]
async fn open_refuses_recovery_lock_before_creating_database() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("jobs.db");
    let _recovery_lock = acquire_recovery_lock(&db_path).expect("hold recovery lock");

    let err = open_sqlite_pool(db_path.to_str().expect("utf8 db path"))
        .await
        .expect_err("active recovery should block open");

    assert!(
        err.to_string().contains("recovery is already in progress"),
        "expected recovery-in-progress error, got: {err}"
    );
    assert!(
        !db_path.exists(),
        "open must acquire the active-owner lock before SQLite creates the database"
    );
}

#[tokio::test]
async fn sqlite_diagnostics_distinguish_lock_file_from_active_owner() {
    reset_sqlite_runtime_health_for_tests();
    let tmp = tempfile::tempdir().expect("tempdir");
    let orphan_db_path = tmp.path().join("orphan.db");
    let orphan_lock_path = active_lock_path(&orphan_db_path);
    std::fs::write(&orphan_lock_path, b"").expect("orphan lock file");

    let orphan_diag = sqlite_diagnostics(&orphan_db_path).await;
    assert_eq!(orphan_diag["active_lock_file_exists"], true);
    assert_eq!(orphan_diag["active_owner_observed"], false);

    let owned_db_path = tmp.path().join("owned.db");
    let pool = open_sqlite_pool(owned_db_path.to_str().expect("utf8 db path"))
        .await
        .expect("open sqlite db");
    let owned_diag = sqlite_diagnostics(&owned_db_path).await;
    pool.close().await;

    assert_eq!(owned_diag["active_lock_file_exists"], true);
    assert_eq!(owned_diag["active_owner_observed"], true);
}

#[test]
fn sqlite_readiness_uses_runtime_health_without_quick_check() {
    reset_sqlite_runtime_health_for_tests();
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("jobs.db");

    let ready = sqlite_readiness(&db_path);
    assert_eq!(ready["ok"], true);
    assert_eq!(ready["check"], "runtime");
    assert!(ready.get("quick_check").is_none());

    std::fs::write(&db_path, b"sqlite will create schema later").expect("placeholder db");
    let not_ready = sqlite_readiness(&db_path);
    assert_eq!(not_ready["ok"], false);
    assert_eq!(not_ready["active_owner_observed"], false);
}

#[test]
fn recovery_refuses_to_rename_database_with_active_owner() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("jobs.db");
    std::fs::write(&db_path, b"corrupt").expect("write corrupt db");

    let active_lock = open_lock_file(&db_path).expect("open active lock");
    active_lock
        .try_lock_shared()
        .expect("hold active owner lock");

    let err = recover_corrupted_database(&db_path, "quick_check failed")
        .expect_err("active owner should block recovery rename");

    assert!(
        err.to_string().contains("active Axon process"),
        "expected active-owner recovery error, got: {err}"
    );
    assert!(
        db_path.exists(),
        "recovery must not rename a database while an active owner holds it"
    );
    assert!(
        tmp.path()
            .read_dir()
            .expect("read temp dir")
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains(".corrupted.")),
        "recovery should not leave a corrupted sidecar when active owner exists"
    );
}

#[test]
fn recovery_refuses_to_rename_database_with_active_owner_in_another_process() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("jobs.db");
    std::fs::write(&db_path, b"corrupt").expect("write corrupt db");

    let active_lock = open_lock_file(&db_path).expect("open active lock");
    active_lock
        .try_lock_shared()
        .expect("hold active owner lock");

    let output = Command::new(std::env::current_exe().expect("current test exe"))
        .args([
            "--ignored",
            "--exact",
            "jobs::store::tests::sqlite_recovery_lock_child_attempts_recovery",
            "--nocapture",
        ])
        .env("AXON_SQLITE_RECOVERY_CHILD_DB", &db_path)
        .output()
        .expect("run child test process");

    assert!(
        output.status.success(),
        "child recovery process failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        db_path.exists(),
        "cross-process recovery must not rename an actively owned database"
    );
    assert!(
        tmp.path()
            .read_dir()
            .expect("read temp dir")
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains(".corrupted.")),
        "cross-process recovery should not leave a corrupted sidecar"
    );
}

#[test]
#[ignore = "child process for cross-process recovery lock test"]
fn sqlite_recovery_lock_child_attempts_recovery() {
    let db_path = std::env::var_os("AXON_SQLITE_RECOVERY_CHILD_DB")
        .map(PathBuf::from)
        .expect("AXON_SQLITE_RECOVERY_CHILD_DB");
    let err = recover_corrupted_database(&db_path, "child quick_check failed")
        .expect_err("active owner in parent should block child recovery");
    assert!(
        err.to_string().contains("active Axon process"),
        "expected active-owner recovery error, got: {err}"
    );
}

#[tokio::test]
async fn sqlite_diagnostics_report_recovery_sidecars_and_runtime_ioerr() {
    reset_sqlite_runtime_health_for_tests();
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("jobs.db");
    let pool = open_sqlite_pool(db_path.to_str().expect("utf8 db path"))
        .await
        .expect("open sqlite db");
    pool.close().await;
    std::fs::write(tmp.path().join("jobs.db.corrupted.9999999999"), b"old").expect("old sidecar");
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(tmp.path().join("jobs.db.corrupted.1"), b"new").expect("new sidecar");

    record_sqlite_runtime_error("error returned from database: (code: 522) disk I/O error");
    let diag = sqlite_diagnostics(&db_path).await;

    assert_eq!(diag["exists"], true);
    assert_eq!(diag["quick_check"], "ok");
    assert_eq!(diag["corrupted_count"], 2);
    let expected_latest = tmp.path().join("jobs.db.corrupted.1");
    assert_eq!(
        diag["latest_corrupted_path"].as_str(),
        Some(expected_latest.to_string_lossy().as_ref())
    );
    assert_eq!(diag["runtime_ioerr_count"], 1);
    assert_eq!(diag["ok"], false);
}

/// A dangling `BEGIN IMMEDIATE` left on a connection that is dropped back into
/// a single-slot pool must NOT poison the slot: the `after_release` ROLLBACK
/// hook should scrub the transaction so the next checkout can `BEGIN IMMEDIATE`
/// again.
///
/// RED without the hook: the second `BEGIN IMMEDIATE` fails with
/// "cannot start a transaction within a transaction".
#[tokio::test]
async fn after_release_hook_scrubs_dangling_transaction() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .after_release(|conn, _meta| Box::pin(rollback_on_release(conn)))
        .connect(":memory:")
        .await
        .expect("pool");

    sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY)")
        .execute(&pool)
        .await
        .expect("create table");

    // Leak a transaction: acquire the single connection, open a write tx, and
    // drop the connection WITHOUT commit/rollback. It returns to the pool still
    // inside the transaction.
    {
        let mut conn = pool.acquire().await.expect("acquire 1");
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .expect("begin immediate");
        // conn dropped here, still in a transaction.
    }

    // The after_release hook must have rolled back, so this BEGIN IMMEDIATE
    // on the recycled connection succeeds.
    let mut conn = pool.acquire().await.expect("acquire 2");
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .expect("second BEGIN IMMEDIATE must succeed — hook should have rolled back the leaked tx");
    sqlx::query("ROLLBACK")
        .execute(&mut *conn)
        .await
        .expect("cleanup rollback");
}

/// Control: WITHOUT the `after_release` hook, the same leak poisons the slot and
/// the second `BEGIN IMMEDIATE` fails — proving the hook is what fixes it.
#[tokio::test]
async fn without_hook_dangling_transaction_poisons_slot() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .expect("pool");

    sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY)")
        .execute(&pool)
        .await
        .expect("create table");

    {
        let mut conn = pool.acquire().await.expect("acquire 1");
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .expect("begin immediate");
    }

    let mut conn = pool.acquire().await.expect("acquire 2");
    let err = sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .expect_err("without the hook the leaked tx must poison the slot");
    assert!(
        err.to_string().contains("within a transaction"),
        "expected nested-transaction error, got: {err}"
    );
}

#[tokio::test]
async fn migration_0014_moves_only_active_result_json_to_progress_json() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .expect("pool");

    for table in [
        "axon_crawl_jobs",
        "axon_embed_jobs",
        "axon_extract_jobs",
        "axon_ingest_jobs",
    ] {
        sqlx::query(&format!(
            "CREATE TABLE {table} (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                result_json TEXT
            )"
        ))
        .execute(&pool)
        .await
        .expect("create pre-0013 table");

        sqlx::query(&format!(
            "INSERT INTO {table} (id, status, result_json) VALUES ('active', 'running', ?)"
        ))
        .bind(r#"{"lifecycle_progress":0.7,"pages_crawled":14}"#)
        .execute(&pool)
        .await
        .expect("insert active row");

        sqlx::query(&format!(
            "INSERT INTO {table} (id, status, result_json) VALUES ('done', 'completed', ?)"
        ))
        .bind(r#"{"pages_crawled":20,"coverage_status":"complete"}"#)
        .execute(&pool)
        .await
        .expect("insert completed row");
    }

    for migration in [
        include_str!("migrations/0013_add_job_progress_json.sql"),
        include_str!("migrations/0014_backfill_active_job_progress_json.sql"),
    ] {
        for statement in migration
            .split(';')
            .map(str::trim)
            .filter(|statement| !statement.is_empty())
        {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("run migration statement");
        }
    }

    for table in [
        "axon_crawl_jobs",
        "axon_embed_jobs",
        "axon_extract_jobs",
        "axon_ingest_jobs",
    ] {
        let (active_progress, active_result): (Option<String>, Option<String>) = sqlx::query_as(
            &format!("SELECT progress_json, result_json FROM {table} WHERE id = 'active'"),
        )
        .fetch_one(&pool)
        .await
        .expect("active row");
        assert_eq!(
            active_progress.as_deref(),
            Some(r#"{"lifecycle_progress":0.7,"pages_crawled":14}"#),
            "{table} should preserve active progress"
        );
        assert_eq!(
            active_result, None,
            "{table} should clear active terminal result"
        );

        let (done_progress, done_result): (Option<String>, Option<String>) = sqlx::query_as(
            &format!("SELECT progress_json, result_json FROM {table} WHERE id = 'done'"),
        )
        .fetch_one(&pool)
        .await
        .expect("completed row");
        assert_eq!(done_progress, None, "{table} should not invent progress");
        assert_eq!(
            done_result.as_deref(),
            Some(r#"{"pages_crawled":20,"coverage_status":"complete"}"#),
            "{table} should preserve terminal result"
        );
    }
}

#[tokio::test]
async fn reclaim_stale_running_jobs_only_reclaims_stale_running_rows() {
    let pool = open_sqlite_pool(":memory:").await.expect("pool");
    let stale_id = Uuid::new_v4().to_string();
    let fresh_id = Uuid::new_v4().to_string();
    let pending_id = Uuid::new_v4().to_string();
    let stale_updated_at = now_ms() - 10_000;
    let fresh_updated_at = now_ms();

    for (id, status, updated_at) in [
        (&stale_id, "running", stale_updated_at),
        (&fresh_id, "running", fresh_updated_at),
        (&pending_id, "pending", stale_updated_at),
    ] {
        sqlx::query(
            "INSERT INTO axon_embed_jobs (id, status, input_text, config_json, created_at, updated_at) \
             VALUES (?, ?, ?, '{}', ?, ?)",
        )
        .bind(id)
        .bind(status)
        .bind("test input")
        .bind(updated_at)
        .bind(updated_at)
        .execute(&pool)
        .await
        .expect("insert job");
    }

    let reclaimed = reclaim_stale_running_jobs_for_table(&pool, JobKind::Embed, 5_000, 0)
        .await
        .expect("reclaim");

    assert_eq!(reclaimed, 1);
    let stale_status: String =
        sqlx::query_scalar("SELECT status FROM axon_embed_jobs WHERE id = ?")
            .bind(&stale_id)
            .fetch_one(&pool)
            .await
            .expect("stale status");
    let fresh_status: String =
        sqlx::query_scalar("SELECT status FROM axon_embed_jobs WHERE id = ?")
            .bind(&fresh_id)
            .fetch_one(&pool)
            .await
            .expect("fresh status");
    let pending_status: String =
        sqlx::query_scalar("SELECT status FROM axon_embed_jobs WHERE id = ?")
            .bind(&pending_id)
            .fetch_one(&pool)
            .await
            .expect("pending status");

    assert_eq!(stale_status, "pending");
    assert_eq!(fresh_status, "running");
    assert_eq!(pending_status, "pending");
}

#[tokio::test]
async fn reclaim_stale_running_jobs_for_table_sets_reclaim_error_text() {
    let pool = open_sqlite_pool(":memory:").await.expect("pool");
    let stale_updated_at = now_ms() - 10_000;

    let stale_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_crawl_jobs \
         (id, status, url, config_json, created_at, updated_at, started_at) \
         VALUES (?, 'running', 'https://stale.example', '{}', ?, ?, ?)",
    )
    .bind(&stale_id)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .execute(&pool)
    .await
    .unwrap();

    let fresh_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_crawl_jobs \
         (id, status, url, config_json, created_at, updated_at) \
         VALUES (?, 'running', 'https://fresh.example', '{}', ?, ?)",
    )
    .bind(&fresh_id)
    .bind(now_ms())
    .bind(now_ms())
    .execute(&pool)
    .await
    .unwrap();

    let pending_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_crawl_jobs \
         (id, status, url, config_json, created_at, updated_at) \
         VALUES (?, 'pending', 'https://pending.example', '{}', ?, ?)",
    )
    .bind(&pending_id)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .execute(&pool)
    .await
    .unwrap();

    let reclaimed = reclaim_stale_running_jobs_for_table(&pool, JobKind::Crawl, 5_000, 0)
        .await
        .expect("reclaim");

    assert_eq!(
        reclaimed, 1,
        "only the stale running row should be reclaimed"
    );

    let (status, error_text, active_attempt_id, last_reclaimed_at): (
        String,
        Option<String>,
        Option<String>,
        Option<i64>,
    ) = sqlx::query_as(
        "SELECT status, error_text, active_attempt_id, last_reclaimed_at \
             FROM axon_crawl_jobs WHERE id = ?",
    )
    .bind(&stale_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "pending");
    assert_eq!(error_text.as_deref(), Some(RECLAIMED_ERROR_TEXT));
    assert_eq!(active_attempt_id, None);
    assert!(last_reclaimed_at.is_some());

    let fresh_status: String =
        sqlx::query_scalar("SELECT status FROM axon_crawl_jobs WHERE id = ?")
            .bind(&fresh_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(fresh_status, "running", "fresh row must not be reclaimed");

    let pending_status: String =
        sqlx::query_scalar("SELECT status FROM axon_crawl_jobs WHERE id = ?")
            .bind(&pending_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending_status, "pending", "pending row must not be touched");
}

/// A stale `running` job whose `attempt_count` has reached `max_attempts` must be
/// dead-lettered (marked `failed`), not re-queued — otherwise a job that hangs
/// every attempt cycles running→pending forever. A sibling that is still under
/// the cap must still be re-queued, and the returned reclaim count must include
/// only the re-queued row.
#[tokio::test]
async fn reclaim_dead_letters_jobs_at_or_above_max_attempts() {
    let pool = open_sqlite_pool(":memory:").await.expect("pool");
    let stale_updated_at = now_ms() - 10_000;

    // Exhausted: already attempted 3 times, cap is 3 → dead-letter.
    let exhausted_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_crawl_jobs \
         (id, status, url, config_json, created_at, updated_at, started_at, attempt_count) \
         VALUES (?, 'running', 'https://exhausted.example', '{}', ?, ?, ?, 3)",
    )
    .bind(&exhausted_id)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .execute(&pool)
    .await
    .unwrap();

    // Under the cap: attempted once → still re-queued.
    let retry_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_crawl_jobs \
         (id, status, url, config_json, created_at, updated_at, started_at, attempt_count) \
         VALUES (?, 'running', 'https://retry.example', '{}', ?, ?, ?, 1)",
    )
    .bind(&retry_id)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .execute(&pool)
    .await
    .unwrap();

    // Over the cap: attempted 4 times, cap is 3 → also dead-lettered (pins the
    // `>=` guard against a future `==`-only regression).
    let over_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_crawl_jobs \
         (id, status, url, config_json, created_at, updated_at, started_at, attempt_count) \
         VALUES (?, 'running', 'https://over.example', '{}', ?, ?, ?, 4)",
    )
    .bind(&over_id)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .execute(&pool)
    .await
    .unwrap();

    let reclaimed = reclaim_stale_running_jobs_for_table(&pool, JobKind::Crawl, 5_000, 3)
        .await
        .expect("reclaim");

    assert_eq!(
        reclaimed, 1,
        "only the under-cap job is re-queued; the two dead-lettered ones are not counted"
    );

    let (exhausted_status, exhausted_error, exhausted_finished): (
        String,
        Option<String>,
        Option<i64>,
    ) = sqlx::query_as("SELECT status, error_text, finished_at FROM axon_crawl_jobs WHERE id = ?")
        .bind(&exhausted_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        exhausted_status, "failed",
        "exhausted job must be dead-lettered"
    );
    assert!(
        exhausted_error
            .as_deref()
            .unwrap_or_default()
            .contains("dead-lettered"),
        "dead-letter error_text should explain the give-up, got: {exhausted_error:?}"
    );
    assert!(
        exhausted_finished.is_some(),
        "dead-lettered job must have finished_at set"
    );

    // The operator-visible progress payload must reflect the terminal failure.
    let exhausted_progress: Option<String> =
        sqlx::query_scalar("SELECT progress_json FROM axon_crawl_jobs WHERE id = ?")
            .bind(&exhausted_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let progress: serde_json::Value =
        serde_json::from_str(exhausted_progress.as_deref().unwrap_or("null"))
            .expect("progress_json should be valid JSON");
    assert_eq!(
        progress["phase"], "failed",
        "dead-letter progress_json phase must be failed"
    );
    assert_eq!(progress["lifecycle_progress"], 1.0);

    let over_status: String = sqlx::query_scalar("SELECT status FROM axon_crawl_jobs WHERE id = ?")
        .bind(&over_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        over_status, "failed",
        "job above the cap (attempts > max) must also be dead-lettered"
    );

    let retry_status: String =
        sqlx::query_scalar("SELECT status FROM axon_crawl_jobs WHERE id = ?")
            .bind(&retry_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(retry_status, "pending", "under-cap job must be re-queued");
}

/// `max_attempts = 0` disables the cap: even a job with a very high
/// `attempt_count` must be re-queued, never dead-lettered.
#[tokio::test]
async fn reclaim_with_unlimited_attempts_never_dead_letters() {
    let pool = open_sqlite_pool(":memory:").await.expect("pool");
    let stale_updated_at = now_ms() - 10_000;

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_crawl_jobs \
         (id, status, url, config_json, created_at, updated_at, started_at, attempt_count) \
         VALUES (?, 'running', 'https://many.example', '{}', ?, ?, ?, 99)",
    )
    .bind(&id)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .execute(&pool)
    .await
    .unwrap();

    let reclaimed = reclaim_stale_running_jobs_for_table(&pool, JobKind::Crawl, 5_000, 0)
        .await
        .expect("reclaim");
    assert_eq!(
        reclaimed, 1,
        "unlimited cap re-queues even a high-attempt job"
    );

    let status: String = sqlx::query_scalar("SELECT status FROM axon_crawl_jobs WHERE id = ?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "pending", "max_attempts=0 must never dead-letter");
}
