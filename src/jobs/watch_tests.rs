use super::*;
use chrono::Utc;
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
