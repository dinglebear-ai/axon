use super::*;
use axon_services::context::ServiceContext;
use chrono::Utc;
use std::sync::Arc;

async fn test_service_context(cfg: &Config) -> ServiceContext {
    ServiceContext::new(Arc::new(cfg.clone()))
        .await
        .expect("service context")
}

#[test]
fn parse_uuid_requires_id() {
    let err = parse_uuid(None, "history").expect_err("missing id should error");
    assert!(err.to_string().contains("watch history requires <id>"));
}

#[test]
fn parse_uuid_rejects_invalid_uuid() {
    let raw = "not-a-uuid".to_string();
    let err = parse_uuid(Some(&raw), "exec").expect_err("invalid uuid should error");
    assert!(err.to_string().contains("invalid character") || err.to_string().contains("UUID"));
}

#[test]
fn parse_watch_runtime_args_rejects_unknown_argument() {
    let err = parse_watch_runtime_args(&[
        "create".to_string(),
        "demo".to_string(),
        "--task-type".to_string(),
        "watch".to_string(),
        "--every-seconds".to_string(),
        "30".to_string(),
        "--bogus".to_string(),
    ])
    .expect_err("unknown argument should error");
    assert!(err.to_string().contains("--bogus"));
}

#[tokio::test]
async fn handle_watch_create_requires_every_seconds() {
    let cfg = Config::test_default();
    let err = handle_watch_create(&cfg, None, "demo".to_string(), "watch".to_string(), 0, None)
        .await
        .expect_err("out-of-bounds interval should error");
    // CLI now shares validate_every_seconds with the HTTP create paths, so the
    // message is the centralized bounds text rather than a CLI-flag-specific one.
    assert!(err.to_string().contains("every_seconds must be between"));
}

#[tokio::test]
async fn handle_watch_create_rejects_invalid_task_payload_json() {
    let cfg = Config::test_default();
    let err = handle_watch_create(
        &cfg,
        None,
        "demo".to_string(),
        "watch".to_string(),
        30,
        Some("{oops".to_string()),
    )
    .await
    .expect_err("invalid json should error");
    assert!(err.to_string().contains("--task-payload is not valid JSON"));
}

#[tokio::test]
async fn run_watch_rejects_unknown_subcommand() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::test_default();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    cfg.positional = vec!["bogus".to_string()];
    let service_context = test_service_context(&cfg).await;
    let err = run_watch(&cfg, &service_context)
        .await
        .expect_err("unknown subcommand should error");
    assert!(err.to_string().contains("bogus"));
}

#[tokio::test]
async fn run_watch_lists_with_sqlite_backend() -> Result<(), Box<dyn Error>> {
    let tmp = tempfile::tempdir()?;
    let mut cfg = Config::default_minimal();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    let service_context = test_service_context(&cfg).await;
    run_watch(&cfg, &service_context).await?;
    Ok(())
}

#[tokio::test]
async fn run_watch_artifacts_lists_with_sqlite_backend() -> Result<(), Box<dyn Error>> {
    let tmp = tempfile::tempdir()?;
    let mut cfg = Config::default_minimal();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    cfg.json_output = true;

    let pool = axon_jobs::store::open_sqlite_pool(&cfg.sqlite_path.to_string_lossy()).await?;
    let watch = axon_jobs::watch::create_watch_def_with_pool(
        &pool,
        &axon_jobs::watch::WatchDefCreate {
            name: "cli-artifacts".to_string(),
            task_type: "watch".to_string(),
            task_payload: serde_json::json!({"urls": ["https://example.com"], "summarize": false}),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now(),
        },
    )
    .await?;
    let run = axon_jobs::watch::create_watch_run_with_pool(&pool, watch.id, None).await?;
    sqlx::query(
        "INSERT INTO axon_watch_run_artifacts (watch_run_id, kind, path, payload, created_at) \
         VALUES (?, 'url-change', NULL, ?, ?)",
    )
    .bind(run.id.to_string())
    .bind(serde_json::json!({"summary": "Changed."}).to_string())
    .bind(axon_jobs::store::now_ms())
    .execute(&pool)
    .await?;

    cfg.positional = vec![
        "artifacts".to_string(),
        run.id.to_string(),
        "--limit".to_string(),
        "10".to_string(),
    ];
    let service_context = test_service_context(&cfg).await;
    run_watch(&cfg, &service_context).await?;
    Ok(())
}

fn source_watch_request() -> watch_svc::WatchRequest {
    watch_svc::WatchRequest {
        source: "https://example.com/docs".to_string(),
        schedule: axon_api::source::WatchSchedule {
            every_seconds: 3600,
            cron: None,
            timezone: None,
        },
        embed: true,
        options: axon_api::source::AdapterOptions::default(),
        scope: None,
        collection: None,
        enabled: Some(true),
    }
}

#[tokio::test]
async fn handle_watch_get_finds_and_reports_missing_source_watches() -> Result<(), Box<dyn Error>> {
    let tmp = tempfile::tempdir()?;
    let mut cfg = Config::default_minimal();
    cfg.sqlite_path = tmp.path().join("jobs.db");

    let store = watch_svc::open_source_watch_store(&cfg, None).await?;
    let created = watch_svc::SourceWatchStoreTrait::create(&store, source_watch_request()).await?;

    handle_watch_get(&cfg, None, &created.watch_id.0).await?;

    let err = handle_watch_get(&cfg, None, "watch_missing")
        .await
        .expect_err("missing watch should error");
    assert!(err.to_string().contains("not found"));
    Ok(())
}

#[tokio::test]
async fn handle_watch_update_pause_resume_and_delete_round_trip() -> Result<(), Box<dyn Error>> {
    let tmp = tempfile::tempdir()?;
    let mut cfg = Config::default_minimal();
    cfg.sqlite_path = tmp.path().join("jobs.db");

    let store = watch_svc::open_source_watch_store(&cfg, None).await?;
    let created = watch_svc::SourceWatchStoreTrait::create(&store, source_watch_request()).await?;
    let id = created.watch_id.0.clone();

    handle_watch_update(
        &cfg,
        None,
        &id,
        watch_svc::WatchUpdateRequest {
            enabled: Some(false),
            schedule: None,
            options: None,
            embed: None,
            collection: None,
            scope: None,
        },
    )
    .await?;

    let paused = watch_svc::SourceWatchStoreTrait::get(&store, watch_svc::WatchId::new(&id))
        .await?
        .expect("watch present after pause");
    assert!(!paused.enabled);

    handle_watch_update(
        &cfg,
        None,
        &id,
        watch_svc::WatchUpdateRequest {
            enabled: Some(true),
            schedule: None,
            options: None,
            embed: None,
            collection: None,
            scope: None,
        },
    )
    .await?;
    let resumed = watch_svc::SourceWatchStoreTrait::get(&store, watch_svc::WatchId::new(&id))
        .await?
        .expect("watch present after resume");
    assert!(resumed.enabled);

    handle_watch_delete(&cfg, None, &id).await?;
    let err = handle_watch_delete(&cfg, None, &id)
        .await
        .expect_err("deleting an already-deleted watch should error");
    assert!(err.to_string().contains("not found"));
    Ok(())
}

#[tokio::test]
async fn run_watch_dispatches_get_update_pause_resume_delete() -> Result<(), Box<dyn Error>> {
    let tmp = tempfile::tempdir()?;
    let mut cfg = Config::default_minimal();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    cfg.json_output = true;

    let store = watch_svc::open_source_watch_store(&cfg, None).await?;
    let created = watch_svc::SourceWatchStoreTrait::create(&store, source_watch_request()).await?;
    let id = created.watch_id.0.clone();

    for args in [
        vec!["get".to_string(), id.clone()],
        vec![
            "update".to_string(),
            id.clone(),
            "--every-seconds".to_string(),
            "120".to_string(),
        ],
        vec!["pause".to_string(), id.clone()],
        vec!["resume".to_string(), id.clone()],
        vec!["delete".to_string(), id.clone()],
    ] {
        cfg.positional = args;
        let service_context = test_service_context(&cfg).await;
        run_watch(&cfg, &service_context).await?;
    }
    Ok(())
}
