use super::*;
use chrono::Utc;
use std::error::Error;
use tempfile::NamedTempFile;

fn lite_cfg(path: &std::path::Path) -> Config {
    let mut cfg = Config::default_lite();
    cfg.sqlite_path = path.to_path_buf();
    cfg
}

#[tokio::test]
async fn lite_watch_create_and_list_round_trip() -> Result<(), Box<dyn Error>> {
    let temp = NamedTempFile::new()?;
    let cfg = lite_cfg(temp.path());
    let created = create_watch_def(
        &cfg,
        &WatchDefCreate {
            name: "lite-watch".to_string(),
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
async fn lite_watch_run_now_records_completed_run() -> Result<(), Box<dyn Error>> {
    let temp = NamedTempFile::new()?;
    let mut cfg = lite_cfg(temp.path());
    cfg.output_dir = std::env::temp_dir().join(format!("axon-watch-lite-{}", Uuid::new_v4()));
    cfg.embed = false;
    let watch = create_watch_def(
        &cfg,
        &WatchDefCreate {
            name: "lite-watch-run".to_string(),
            task_type: "refresh".to_string(),
            task_payload: serde_json::json!({"urls":["https://example.com"]}),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now(),
        },
    )
    .await?;

    let run = run_watch_now(&cfg, &watch).await?;
    assert_eq!(run.watch_id, watch.id);
    assert_eq!(run.status, WATCH_RUN_STATUS_COMPLETED);
    Ok(())
}
