use super::*;
use axon_jobs::backend::{JobBackend, JobKind, JobPayload};
use axon_jobs::ops::{enqueue_job, mark_completed, mark_failed};
use axon_jobs::store::open_sqlite_pool;
use sqlx::SqlitePool;
use std::time::Duration;
use uuid::Uuid;

async fn fresh_pool() -> SqlitePool {
    open_sqlite_pool(":memory:").await.expect("pool")
}

/// has_active_jobs is per-kind: a pending row in another table must NOT make
/// us think the queried kind has active jobs. (bd axon_rust-cr5.14)
#[tokio::test]
async fn has_active_jobs_is_isolated_per_kind() {
    let pool = fresh_pool().await;
    // Seed a pending crawl job.
    enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: "https://example.com".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .expect("enqueue crawl");

    // Seed a pending embed job.
    enqueue_job(
        &pool,
        &JobPayload::Embed {
            input: "doc".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .expect("enqueue embed");

    let active_crawl = has_active_for_kind(&pool, JobKind::Crawl).await;
    let active_embed = has_active_for_kind(&pool, JobKind::Embed).await;
    let active_extract = has_active_for_kind(&pool, JobKind::Extract).await;

    assert!(active_crawl, "crawl table has pending row");
    assert!(active_embed, "embed table has pending row");
    assert!(
        !active_extract,
        "extract table is empty — must not be considered active"
    );
}

/// Once all jobs for a kind reach a terminal state (completed/failed/canceled),
/// has_active_jobs returns false even if other kinds still have pending rows.
#[tokio::test]
async fn has_active_jobs_false_after_terminal_states() {
    let pool = fresh_pool().await;
    let crawl_id = enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: "https://example.com".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .expect("enqueue crawl");
    // Seed an unrelated pending embed row that should NOT block the crawl drain.
    let _ = enqueue_job(
        &pool,
        &JobPayload::Embed {
            input: "doc".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .expect("enqueue embed");

    // Move crawl to running, then completed.
    axon_jobs::ops::claim_next_pending(&pool, JobKind::Crawl)
        .await
        .expect("claim crawl");
    mark_completed(&pool, JobKind::Crawl, crawl_id, None)
        .await
        .expect("complete crawl");

    let active_crawl = has_active_for_kind(&pool, JobKind::Crawl).await;
    let active_embed = has_active_for_kind(&pool, JobKind::Embed).await;
    assert!(
        !active_crawl,
        "crawl drain should see no active rows once completed"
    );
    assert!(active_embed, "embed remains pending — unrelated kind");
}

/// Bounded-time drain: once the queried kind has no pending/running rows,
/// the wait loop returns within ~1s even if other kinds still have rows.
#[tokio::test]
async fn drain_terminates_quickly_on_terminal_state() {
    let pool = fresh_pool().await;
    let id = enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: "https://example.com".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .expect("enqueue");
    // Seed unrelated pending embed row that must not stall the crawl drain.
    let _ = enqueue_job(
        &pool,
        &JobPayload::Embed {
            input: "x".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .expect("enqueue embed");

    // Mark crawl failed (terminal).
    axon_jobs::ops::claim_next_pending(&pool, JobKind::Crawl)
        .await
        .expect("claim");
    mark_failed(&pool, JobKind::Crawl, id, "test")
        .await
        .expect("fail");

    // Simulate the run_worker drain wait: should return immediately for crawl.
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        let mut iters = 0;
        loop {
            if !has_active_for_kind(&pool, JobKind::Crawl).await {
                break iters;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            iters += 1;
            if iters > 40 {
                break iters;
            }
        }
    })
    .await
    .expect("drain wait must not hang past 2s");
    assert!(
        result < 5,
        "drain should exit immediately for terminal-state crawl, got {result} iters"
    );
}

#[tokio::test]
async fn start_worker_requires_in_process_workers() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::test_default();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    let cfg = Arc::new(cfg);
    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .expect("backend");
    let runtime = SqliteServiceRuntime {
        cfg,
        backend: Arc::new(backend),
    };

    let err = ServiceJobRuntime::start_worker(&runtime, JobKind::Crawl)
        .await
        .expect_err("enqueue-only runtime cannot start worker drain");
    assert!(
        err.to_string().contains("no in-process workers running"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn drain_jobs_times_out_when_queue_stays_active() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::test_default();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    cfg.job_wait_timeout_secs = 1;
    let cfg = Arc::new(cfg);
    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .expect("backend");
    backend
        .enqueue(JobPayload::Crawl {
            url: "https://example.com".into(),
            config_json: "{}".into(),
        })
        .await
        .expect("enqueue active job");
    let runtime = SqliteServiceRuntime {
        cfg: Arc::clone(&cfg),
        backend: Arc::new(backend),
    };

    let err = tokio::time::timeout(
        Duration::from_secs(3),
        ServiceJobRuntime::drain_jobs(&runtime, JobKind::Crawl),
    )
    .await
    .expect("drain should return before outer timeout")
    .expect_err("active queue should hit configured drain timeout");

    assert!(
        err.to_string().contains("timed out"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn sqlite_runtime_exposes_backend_pool_for_shared_watch_callers() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::test_default();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    let cfg = Arc::new(cfg);
    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .expect("backend");
    let expected_pool = Arc::clone(backend.pool());
    let runtime = SqliteServiceRuntime {
        cfg,
        backend: Arc::new(backend),
    };

    let shared_pool =
        ServiceJobRuntime::sqlite_pool(&runtime).expect("sqlite runtime should expose shared pool");
    assert!(
        Arc::ptr_eq(&shared_pool, &expected_pool),
        "runtime must expose the backend pool instead of opening a second pool"
    );
}

#[tokio::test]
async fn sqlite_runtime_rejects_negative_job_pagination() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::test_default();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    let cfg = Arc::new(cfg);
    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .expect("backend");
    let runtime = SqliteServiceRuntime {
        cfg,
        backend: Arc::new(backend),
    };

    let limit_err = ServiceJobRuntime::list_jobs(&runtime, JobKind::Crawl, -1, 0)
        .await
        .expect_err("negative limit should be rejected");
    assert!(limit_err.to_string().contains("limit must be non-negative"));

    let offset_err = ServiceJobRuntime::list_jobs(&runtime, JobKind::Crawl, 10, -1)
        .await
        .expect_err("negative offset should be rejected");
    assert!(
        offset_err
            .to_string()
            .contains("offset must be non-negative")
    );
}

/// Mirror of SqliteServiceRuntime::has_active_jobs that operates on a raw
/// pool — lets tests exercise the same predicate without constructing a
/// full SqliteJobBackend.
async fn has_active_for_kind(pool: &SqlitePool, kind: JobKind) -> bool {
    let table = kind.table_name();
    let sql = format!(
        "SELECT EXISTS(SELECT 1 FROM {} WHERE status IN ('pending','running') LIMIT 1)",
        table
    );
    sqlx::query_scalar::<_, bool>(&sql)
        .fetch_one(pool)
        .await
        .unwrap_or(false)
}

// Silence "unused" lints when only the helper is built without the tests.
#[allow(dead_code)]
fn _force_uuid_use() {
    let _ = Uuid::new_v4();
}
