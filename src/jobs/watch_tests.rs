use super::*;
use chrono::{Duration, Utc};
use std::error::Error;
use tempfile::NamedTempFile;

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
            task_type: "refresh".to_string(),
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

#[test]
fn validate_task_type_accepts_supported_and_rejects_others() {
    assert!(validate_task_type("refresh").is_ok());
    assert!(validate_task_type("crawl").is_err());
    assert!(validate_task_type("").is_err());
    // Surrounding whitespace is rejected — the stored value would otherwise
    // fail the verbatim dispatch match and the watch could never run.
    assert!(validate_task_type(" refresh").is_err());
    assert!(validate_task_type("refresh ").is_err());
}

#[tokio::test]
async fn lease_due_watches_leases_due_skips_future_and_already_leased() -> Result<(), Box<dyn Error>>
{
    let temp = NamedTempFile::new()?;
    let cfg = sqlite_cfg(temp.path());
    let pool = crate::jobs::store::open_sqlite_pool(&temp.path().to_string_lossy()).await?;

    let make = |name: &str, next_run: chrono::DateTime<Utc>| WatchDefCreate {
        name: name.to_string(),
        task_type: "refresh".to_string(),
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
            task_type: "refresh".to_string(),
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
            task_type: "refresh".to_string(),
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
