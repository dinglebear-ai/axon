use super::*;
use chrono::{Duration, Utc};
use std::error::Error;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration as StdDuration;
use tempfile::NamedTempFile;

struct LoopbackReset;

impl LoopbackReset {
    fn enable() -> Self {
        crate::core::http::set_allow_loopback(true);
        Self
    }
}

impl Drop for LoopbackReset {
    fn drop(&mut self) {
        crate::core::http::set_allow_loopback(false);
    }
}

fn sqlite_cfg(path: &std::path::Path) -> Config {
    let mut cfg = Config::default_minimal();
    cfg.sqlite_path = path.to_path_buf();
    cfg
}

#[tokio::test]
async fn sqlite_watch_create_and_list_round_trip() -> Result<(), Box<dyn Error>> {
    let temp = NamedTempFile::new()?;
    let cfg = sqlite_cfg(temp.path());
    let created = create_watch_def(
        &cfg,
        &WatchDefCreate {
            name: "sqlite-watch".to_string(),
            task_type: "watch".to_string(),
            task_payload: serde_json::json!({"urls":["https://example.com"]}),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now(),
        },
    )
    .await?;

    let listed = list_watch_defs(&cfg, 20).await?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);
    Ok(())
}

#[tokio::test]
async fn sqlite_watch_create_trims_name_at_write_boundary() -> Result<(), Box<dyn Error>> {
    let temp = NamedTempFile::new()?;
    let pool = crate::jobs::store::open_sqlite_pool(&temp.path().to_string_lossy()).await?;

    let created = create_watch_def_with_pool(
        &pool,
        &WatchDefCreate {
            name: "  spaced-watch  ".to_string(),
            task_type: "watch".to_string(),
            task_payload: serde_json::json!({"urls": ["https://example.com"]}),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now(),
        },
    )
    .await?;

    assert_eq!(created.name, "spaced-watch");
    let stored = get_watch_def_with_pool(&pool, created.id)
        .await?
        .expect("stored watch");
    assert_eq!(stored.name, "spaced-watch");
    Ok(())
}

#[tokio::test]
async fn sqlite_watch_create_rejects_invalid_input_at_write_boundary() -> Result<(), Box<dyn Error>>
{
    let temp = NamedTempFile::new()?;
    let pool = crate::jobs::store::open_sqlite_pool(&temp.path().to_string_lossy()).await?;

    let invalid = WatchDefCreate {
        name: "bad-watch".to_string(),
        task_type: "crawl".to_string(),
        task_payload: serde_json::json!({"urls": ["https://example.com"]}),
        every_seconds: 60,
        enabled: true,
        next_run_at: Utc::now(),
    };

    let err = create_watch_def_with_pool(&pool, &invalid)
        .await
        .expect_err("invalid task_type must be rejected before insert");
    assert!(err.to_string().contains("unsupported task_type"));

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM axon_watch_defs")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 0, "invalid watch must not be persisted");
    Ok(())
}

#[test]
fn validate_task_type_accepts_supported_and_rejects_others() {
    assert!(validate_task_type("watch").is_ok());
    assert!(validate_task_type("crawl").is_err());
    assert!(validate_task_type("").is_err());
    // Surrounding whitespace is rejected — the stored value would otherwise
    // fail the verbatim dispatch match and the watch could never run.
    assert!(validate_task_type(" watch").is_err());
    assert!(validate_task_type("watch ").is_err());
}

#[test]
fn validate_every_seconds_enforces_shared_bounds() {
    assert!(validate_every_seconds(MIN_WATCH_INTERVAL_SECS).is_ok());
    assert!(validate_every_seconds(MAX_WATCH_INTERVAL_SECS).is_ok());
    assert!(validate_every_seconds(3600).is_ok());
    // Below minimum (the `/v1/watch` gap this centralization closes) and above max.
    assert!(validate_every_seconds(MIN_WATCH_INTERVAL_SECS - 1).is_err());
    assert!(validate_every_seconds(1).is_err());
    assert!(validate_every_seconds(0).is_err());
    assert!(validate_every_seconds(MAX_WATCH_INTERVAL_SECS + 1).is_err());
}

#[test]
fn watch_create_request_builds_shared_defaults() -> Result<(), Box<dyn Error>> {
    let before = Utc::now();
    let input = WatchDefCreateRequest {
        name: "  shared-defaults  ".to_string(),
        task_type: "watch".to_string(),
        task_payload: serde_json::json!({"urls": ["https://example.com"]}),
        every_seconds: 60,
        enabled: None,
        next_run_at: None,
    }
    .into_create()?;
    let after = Utc::now();

    assert_eq!(input.name, "shared-defaults");
    assert!(input.enabled);
    assert!(input.next_run_at >= before + Duration::seconds(60));
    assert!(input.next_run_at <= after + Duration::seconds(60));
    Ok(())
}

#[test]
fn watch_create_request_rejects_invalid_interval_before_defaulting_next_run() {
    let err = WatchDefCreateRequest {
        name: "bad-interval".to_string(),
        task_type: "watch".to_string(),
        task_payload: serde_json::json!({"urls": ["https://example.com"]}),
        every_seconds: i64::MAX,
        enabled: None,
        next_run_at: None,
    }
    .into_create()
    .expect_err("invalid interval should return validation error");
    assert!(err.contains("every_seconds must be between"));
}

#[test]
fn validate_task_payload_accepts_valid() {
    let p = serde_json::json!({
        "urls": ["https://example.com/a", "https://example.com/b"],
        "ignore_patterns": ["^Last updated:"],
        "max_depth": 3
    });
    assert!(validate_task_payload(&p).is_ok());
}

#[test]
fn validate_task_payload_rejects_empty_urls() {
    let p = serde_json::json!({ "urls": [] });
    assert!(validate_task_payload(&p).is_err());
}

#[test]
fn validate_task_payload_rejects_non_string_url() {
    let p = serde_json::json!({ "urls": ["https://example.com", 42] });
    assert!(validate_task_payload(&p).is_err());
}

#[test]
fn validate_task_payload_rejects_too_many_urls() {
    let urls: Vec<String> = (0..=MAX_WATCH_URLS)
        .map(|i| format!("https://example.com/{i}"))
        .collect();
    let p = serde_json::json!({ "urls": urls });
    assert!(validate_task_payload(&p).is_err());
}

#[test]
fn validate_task_payload_rejects_bad_ignore_regex() {
    let p = serde_json::json!({ "urls": ["https://example.com"], "ignore_patterns": ["("] });
    assert!(validate_task_payload(&p).is_err());
}

#[test]
fn validate_task_payload_rejects_over_limit_max_depth() {
    let p =
        serde_json::json!({ "urls": ["https://example.com"], "max_depth": MAX_WATCH_DEPTH + 1 });
    assert!(validate_task_payload(&p).is_err());
}

#[tokio::test]
async fn lease_advances_next_run_at_for_single_flight() -> Result<(), Box<dyn Error>> {
    // A run that outlives its lease TTL must not be re-leased: leasing advances
    // next_run_at to now + every_seconds, so the row is no longer due even after
    // the lease expires.
    let temp = NamedTempFile::new()?;
    let pool = crate::jobs::store::open_sqlite_pool(&temp.path().to_string_lossy()).await?;

    let due = create_watch_def_with_pool(
        &pool,
        &WatchDefCreate {
            name: "long-runner".to_string(),
            task_type: "watch".to_string(),
            task_payload: serde_json::json!({"urls": ["https://example.com"]}),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now() - Duration::seconds(10),
        },
    )
    .await?;

    let now = now_ms();
    // Short 1s lease TTL to model a run that outlives its lease.
    let leased = lease_due_watches(&pool, now, 1_000, 16).await?;
    assert_eq!(leased.len(), 1);
    assert_eq!(leased[0].id, due.id);

    // Sweep again well after the lease has expired (now + 5s > now + 1s TTL) but
    // before next_run_at (now + 60s). The advanced next_run_at must block re-lease.
    let after_expiry = now + 5_000;
    let again = lease_due_watches(&pool, after_expiry, 1_000, 16).await?;
    assert!(
        again.is_empty(),
        "expired lease must not re-fire an in-flight watch — next_run_at was advanced"
    );

    let row = get_watch_def_with_pool(&pool, due.id).await?.expect("def");
    assert!(
        row.next_run_at.timestamp_millis() >= now + 60_000,
        "next_run_at advanced by every_seconds at lease time"
    );
    Ok(())
}

#[tokio::test]
async fn lease_due_watches_leases_due_skips_future_and_already_leased() -> Result<(), Box<dyn Error>>
{
    let temp = NamedTempFile::new()?;
    let cfg = sqlite_cfg(temp.path());
    let pool = crate::jobs::store::open_sqlite_pool(&temp.path().to_string_lossy()).await?;

    let make = |name: &str, next_run: DateTime<Utc>| WatchDefCreate {
        name: name.to_string(),
        task_type: "watch".to_string(),
        task_payload: serde_json::json!({"urls": ["https://example.com"]}),
        every_seconds: 60,
        enabled: true,
        next_run_at: next_run,
    };

    let due =
        create_watch_def_with_pool(&pool, &make("due", Utc::now() - Duration::seconds(10))).await?;
    let _future =
        create_watch_def_with_pool(&pool, &make("future", Utc::now() + Duration::hours(1))).await?;
    let _ = cfg; // sqlite_cfg only used to anchor the temp path lifetime

    let now = now_ms();
    let leased = lease_due_watches(&pool, now, 300_000, 16).await?;
    assert_eq!(leased.len(), 1, "only the due watch should be leased");
    assert_eq!(leased[0].id, due.id);
    assert!(leased[0].lease_expires_at.is_some());

    // A second sweep at the same instant must NOT re-lease the held watch.
    let again = lease_due_watches(&pool, now, 300_000, 16).await?;
    assert!(again.is_empty(), "an active lease blocks re-leasing");

    // Once the run finishes, the lease clears and next_run_at moves forward.
    let run = create_watch_run_with_pool(&pool, due.id, None).await?;
    finish_watch_run_with_pool(
        &pool,
        due.id,
        run.id,
        WATCH_RUN_STATUS_COMPLETED,
        Some(&serde_json::json!({"ok": true})),
        None,
    )
    .await?;
    let after = get_watch_def_with_pool(&pool, due.id).await?.expect("def");
    assert!(after.lease_expires_at.is_none(), "finish clears the lease");
    assert!(after.next_run_at > due.next_run_at, "next_run_at advances");
    Ok(())
}

#[tokio::test]
async fn lease_due_watches_skips_disabled() -> Result<(), Box<dyn Error>> {
    let temp = NamedTempFile::new()?;
    let pool = crate::jobs::store::open_sqlite_pool(&temp.path().to_string_lossy()).await?;
    create_watch_def_with_pool(
        &pool,
        &WatchDefCreate {
            name: "disabled".to_string(),
            task_type: "watch".to_string(),
            task_payload: serde_json::json!({"urls": ["https://example.com"]}),
            every_seconds: 60,
            enabled: false,
            next_run_at: Utc::now() - Duration::seconds(10),
        },
    )
    .await?;
    let leased = lease_due_watches(&pool, now_ms(), 300_000, 16).await?;
    assert!(leased.is_empty(), "disabled watches are never leased");
    Ok(())
}

#[tokio::test]
async fn run_now_rejects_watch_with_active_lease() -> Result<(), Box<dyn Error>> {
    let temp = NamedTempFile::new()?;
    let mut cfg = sqlite_cfg(temp.path());
    cfg.embed = false;
    let pool = crate::jobs::store::open_sqlite_pool(&temp.path().to_string_lossy()).await?;
    let watch = create_watch_def_with_pool(
        &pool,
        &WatchDefCreate {
            name: "manual-single-flight".to_string(),
            task_type: "watch".to_string(),
            task_payload: serde_json::json!({"urls": ["https://example.com"], "summarize": false}),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now() - Duration::seconds(10),
        },
    )
    .await?;

    let leased = lease_due_watches(&pool, now_ms(), 300_000, 16).await?;
    assert_eq!(leased.len(), 1);

    let err = run_watch_now_with_pool(&cfg, &pool, &watch)
        .await
        .expect_err("manual run should respect active watch lease");
    assert!(err.to_string().contains("already running"));

    let run_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM axon_watch_runs")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        run_count, 0,
        "manual run should not create a run row when lease is active"
    );
    Ok(())
}

#[tokio::test]
async fn sqlite_watch_run_now_records_completed_run() -> Result<(), Box<dyn Error>> {
    // Spider's async call chain is deep enough in debug builds to overflow the default
    // tokio current_thread stack. Spawn on an OS thread with explicit stack headroom.
    let temp = NamedTempFile::new()?;
    let mut cfg = sqlite_cfg(temp.path());
    cfg.output_dir = std::env::temp_dir().join(format!("axon-watch-sqlite-{}", Uuid::new_v4()));
    cfg.embed = false;
    let watch = create_watch_def(
        &cfg,
        &WatchDefCreate {
            name: "sqlite-watch-run".to_string(),
            task_type: "watch".to_string(),
            task_payload: serde_json::json!({"urls":["https://example.com"]}),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now(),
        },
    )
    .await?;

    let (cfg_c, watch_c) = (cfg.clone(), watch.clone());
    let run = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Runtime::new()
                .expect("tokio runtime")
                .block_on(run_watch_now(&cfg_c, &watch_c))
                .map_err(|e| e.to_string())
        })
        .expect("thread spawn")
        .join()
        .expect("thread joined")
        .map_err(|e| -> Box<dyn Error> { e.into() })?;
    assert_eq!(run.watch_id, watch.id);
    assert_eq!(run.status, WATCH_RUN_STATUS_COMPLETED);
    Ok(())
}

#[tokio::test]
async fn watch_first_run_seeds_crawl_and_writes_artifact() -> Result<(), Box<dyn Error>> {
    let temp = NamedTempFile::new()?;
    let mut cfg = sqlite_cfg(temp.path());
    cfg.output_dir = std::env::temp_dir().join(format!("axon-watch-cd-{}", Uuid::new_v4()));
    cfg.embed = false;
    let watch = create_watch_def(
        &cfg,
        &WatchDefCreate {
            name: "cd-seed".into(),
            task_type: "watch".into(),
            task_payload: serde_json::json!({"urls": ["https://example.com/"], "summarize": false}),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now(),
        },
    )
    .await?;

    let (cfg_c, watch_c) = (cfg.clone(), watch.clone());
    let run = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(run_watch_now(&cfg_c, &watch_c))
                .map_err(|e| e.to_string())
        })
        .unwrap()
        .join()
        .unwrap()
        .map_err(|e| -> Box<dyn Error> { e.into() })?;
    assert_eq!(run.status, WATCH_RUN_STATUS_COMPLETED);

    let pool = crate::jobs::store::open_sqlite_pool(&temp.path().to_string_lossy()).await?;
    let crawls: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM axon_crawl_jobs")
        .fetch_one(&pool)
        .await?;
    assert_eq!(crawls, 1, "first run seeds one crawl");
    let arts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM axon_watch_run_artifacts WHERE kind='url-change'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(arts, 1, "first run writes one change artifact");
    assert_eq!(
        run.result_json
            .as_ref()
            .and_then(|j| j.get("changed"))
            .and_then(|v| v.as_u64()),
        Some(1)
    );
    Ok(())
}

#[tokio::test]
async fn sqlite_watch_artifacts_list_round_trip() -> Result<(), Box<dyn Error>> {
    let temp = NamedTempFile::new()?;
    let pool = crate::jobs::store::open_sqlite_pool(&temp.path().to_string_lossy()).await?;
    let watch = create_watch_def_with_pool(
        &pool,
        &WatchDefCreate {
            name: "artifact-watch".into(),
            task_type: "watch".into(),
            task_payload: serde_json::json!({"urls": ["https://example.com"], "summarize": false}),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now(),
        },
    )
    .await?;
    let run = create_watch_run_with_pool(&pool, watch.id, None).await?;
    sqlx::query(
        "INSERT INTO axon_watch_run_artifacts (watch_run_id, kind, path, payload, created_at) \
         VALUES (?, 'url-change', NULL, ?, ?)",
    )
    .bind(run.id.to_string())
    .bind(serde_json::json!({"url": "https://example.com", "summary": "Changed."}).to_string())
    .bind(now_ms())
    .execute(&pool)
    .await?;

    let artifacts = list_watch_run_artifacts_with_pool(&pool, run.id, 10).await?;
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].watch_run_id, run.id);
    assert_eq!(artifacts[0].kind, "url-change");
    assert_eq!(artifacts[0].payload["summary"], "Changed.");

    let clamped = list_watch_run_artifacts_with_pool(&pool, run.id, -1).await?;
    assert_eq!(
        clamped.len(),
        1,
        "negative limits should clamp, not go unbounded"
    );
    Ok(())
}

struct MutablePageServer {
    url: String,
    body: Arc<Mutex<String>>,
    stop: Arc<AtomicBool>,
    addr: SocketAddr,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for MutablePageServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn spawn_mutable_page(initial: &str) -> Result<MutablePageServer, Box<dyn Error>> {
    let body = Arc::new(Mutex::new(initial.to_string()));
    let stop = Arc::new(AtomicBool::new(false));
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?;
    let body_for_thread = Arc::clone(&body);
    let stop_for_thread = Arc::clone(&stop);
    let handle = std::thread::spawn(move || {
        while !stop_for_thread.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buf = [0_u8; 1024];
                    let _ = stream.read(&mut buf);
                    let body = body_for_thread.lock().expect("page body lock").clone();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(StdDuration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });
    Ok(MutablePageServer {
        url: format!("http://{addr}/"),
        body,
        stop,
        addr,
        handle: Some(handle),
    })
}

fn run_watch_now_on_large_stack(
    cfg: Config,
    db_path: std::path::PathBuf,
    watch: WatchDef,
) -> Result<WatchRun, Box<dyn Error>> {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let _loopback = LoopbackReset::enable();
            tokio::runtime::Runtime::new()
                .expect("tokio runtime")
                .block_on(async {
                    let pool = crate::jobs::store::open_sqlite_pool(&db_path.to_string_lossy())
                        .await
                        .map_err(|e| e.to_string())?;
                    let run = run_watch_now_with_pool(&cfg, &pool, &watch)
                        .await
                        .map_err(|e| e.to_string());
                    pool.close().await;
                    run
                })
        })
        .expect("thread spawn")
        .join()
        .expect("thread joined")
        .map_err(|e| -> Box<dyn Error> { e.into() })
}

#[tokio::test]
async fn live_watch_only_recrawls_when_page_changes() -> Result<(), Box<dyn Error>> {
    let _loopback = LoopbackReset::enable();
    let server = spawn_mutable_page("Welcome to the docs.\nVersion one content.")?;

    let temp = NamedTempFile::new()?;
    let mut cfg = sqlite_cfg(temp.path());
    cfg.output_dir = std::env::temp_dir().join(format!("axon-watch-live-{}", Uuid::new_v4()));
    cfg.embed = false;
    let pool = crate::jobs::store::open_sqlite_pool(&temp.path().to_string_lossy()).await?;
    let watch = create_watch_def_with_pool(
        &pool,
        &WatchDefCreate {
            name: "live-change-proof".into(),
            task_type: "watch".into(),
            task_payload: serde_json::json!({
                "urls": [server.url.clone()],
                "summarize": false,
                "change_threshold_words": 0
            }),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now(),
        },
    )
    .await?;

    let first =
        run_watch_now_on_large_stack(cfg.clone(), temp.path().to_path_buf(), watch.clone())?;
    let crawl_count_after_seed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM axon_crawl_jobs")
        .fetch_one(&pool)
        .await?;
    assert_eq!(crawl_count_after_seed, 1, "first run seeds one crawl");
    assert_eq!(
        first
            .result_json
            .as_ref()
            .and_then(|j| j.get("changed"))
            .and_then(|v| v.as_u64()),
        Some(1)
    );

    let second =
        run_watch_now_on_large_stack(cfg.clone(), temp.path().to_path_buf(), watch.clone())?;
    let crawl_count_after_unchanged: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM axon_crawl_jobs")
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        crawl_count_after_unchanged, crawl_count_after_seed,
        "unchanged page must not enqueue a second crawl"
    );
    assert_eq!(
        second
            .result_json
            .as_ref()
            .and_then(|j| j.get("changed"))
            .and_then(|v| v.as_u64()),
        Some(0)
    );

    sqlx::query("UPDATE axon_crawl_jobs SET status = ?, finished_at = ?, updated_at = ?")
        .bind(crate::jobs::status::JobStatus::Completed.as_str())
        .bind(now_ms())
        .bind(now_ms())
        .execute(&pool)
        .await?;

    *server.body.lock().expect("page body lock") =
        "Welcome to the docs.\nVersion two content with a new release note.".to_string();
    let third =
        run_watch_now_on_large_stack(cfg.clone(), temp.path().to_path_buf(), watch.clone())?;
    let crawl_count_after_change: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM axon_crawl_jobs")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        crawl_count_after_change,
        crawl_count_after_seed + 1,
        "changed page must enqueue exactly one additional crawl"
    );
    assert_eq!(
        third
            .result_json
            .as_ref()
            .and_then(|j| j.get("changed"))
            .and_then(|v| v.as_u64()),
        Some(1)
    );

    Ok(())
}
