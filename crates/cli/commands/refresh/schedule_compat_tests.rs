use super::schedule::handle_refresh_schedule;
use crate::crates::jobs::common::{make_pool, resolve_test_pg_url, test_config};
use crate::crates::jobs::watch::list_watch_defs_with_pool;
use std::error::Error;

#[tokio::test]
async fn refresh_schedule_add_creates_watch_def_with_task_refresh() -> Result<(), Box<dyn Error>> {
    let Some(pg_url) = resolve_test_pg_url() else {
        return Ok(());
    };
    let mut cfg = test_config(&pg_url);
    let name = format!("refresh-compat-{}", uuid::Uuid::new_v4());
    cfg.positional = vec![
        "schedule".to_string(),
        "add".to_string(),
        name.clone(),
        "https://example.com".to_string(),
        "--every-seconds".to_string(),
        "300".to_string(),
    ];
    if make_pool(&cfg).await.is_err() {
        return Ok(());
    }

    handle_refresh_schedule(&cfg).await?;

    let pool = make_pool(&cfg).await?;
    let defs = list_watch_defs_with_pool(&pool, 500).await?;
    let def = defs
        .into_iter()
        .find(|d| d.name == name)
        .ok_or("missing watch def for refresh schedule")?;
    assert_eq!(def.task_type, "refresh");

    let _ = sqlx::query("DELETE FROM axon_watch_defs WHERE id=$1")
        .bind(def.id)
        .execute(&pool)
        .await?;
    let _ = sqlx::query("DELETE FROM axon_refresh_schedules WHERE name=$1")
        .bind(name)
        .execute(&pool)
        .await?;
    Ok(())
}

#[tokio::test]
async fn refresh_schedule_list_reads_from_watch_defs_refresh() -> Result<(), Box<dyn Error>> {
    let Some(pg_url) = resolve_test_pg_url() else {
        return Ok(());
    };
    let mut cfg = test_config(&pg_url);
    let name = format!("refresh-compat-list-{}", uuid::Uuid::new_v4());
    cfg.positional = vec![
        "schedule".to_string(),
        "add".to_string(),
        name.clone(),
        "https://example.com".to_string(),
        "--every-seconds".to_string(),
        "300".to_string(),
    ];
    if make_pool(&cfg).await.is_err() {
        return Ok(());
    }
    handle_refresh_schedule(&cfg).await?;

    cfg.positional = vec!["schedule".to_string(), "list".to_string()];
    handle_refresh_schedule(&cfg).await?;

    let pool = make_pool(&cfg).await?;
    let defs = list_watch_defs_with_pool(&pool, 500).await?;
    assert!(
        defs.iter()
            .any(|d| d.name == name && d.task_type == "refresh")
    );

    let _ = sqlx::query("DELETE FROM axon_watch_defs WHERE name=$1")
        .bind(&name)
        .execute(&pool)
        .await?;
    let _ = sqlx::query("DELETE FROM axon_refresh_schedules WHERE name=$1")
        .bind(&name)
        .execute(&pool)
        .await?;
    Ok(())
}

#[test]
fn refresh_schedule_run_due_uses_service_refresh_start() {
    let source = include_str!("../../../services/refresh_schedule.rs");
    assert!(
        source.contains("match refresh_start(cfg, &urls).await"),
        "schedule run-due path should dispatch via services::refresh::refresh_start inside services"
    );
}

#[test]
fn watch_run_now_refresh_task_uses_service_layer() {
    let source = include_str!("../watch.rs");
    assert!(
        source.contains("watch_svc::run_watch_now"),
        "watch run-now should delegate through services::watch::run_watch_now"
    );
    assert!(
        !source.contains("refresh_service::refresh_start"),
        "watch CLI should not dispatch refresh directly"
    );
}
