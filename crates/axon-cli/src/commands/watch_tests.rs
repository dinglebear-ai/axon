use super::*;
use axon_services::context::ServiceContext;
use std::sync::Arc;

async fn test_service_context(cfg: &Config) -> ServiceContext {
    ServiceContext::new(Arc::new(cfg.clone()))
        .await
        .expect("service context")
}

#[test]
fn parse_watch_runtime_args_rejects_unknown_argument() {
    let err = parse_watch_runtime_args(&[
        "create".to_string(),
        "https://example.com/demo".to_string(),
        "--every-seconds".to_string(),
        "30".to_string(),
        "--bogus".to_string(),
    ])
    .expect_err("unknown argument should error");
    assert!(err.to_string().contains("--bogus"));
}

#[test]
fn parse_watch_runtime_args_rejects_removed_artifacts_subcommand() {
    let err = parse_watch_runtime_args(&[
        "artifacts".to_string(),
        "00000000-0000-0000-0000-000000000000".to_string(),
    ])
    .expect_err("removed artifacts command should error");
    assert!(err.to_string().contains("artifacts"));
}

#[tokio::test]
async fn handle_watch_create_requires_every_seconds() {
    let cfg = Config::test_default();
    let err = handle_watch_create(&cfg, None, "https://example.com/demo".to_string(), 0, None)
        .await
        .expect_err("out-of-bounds interval should error");
    // CLI now shares validate_every_seconds with the HTTP create paths, so the
    // message is the centralized bounds text rather than a CLI-flag-specific one.
    assert!(err.to_string().contains("every_seconds must be between"));
}

#[test]
fn source_from_arg_trims_and_rejects_empty_sources() {
    let source = source_from_arg("  https://example.com/from-name  ")
        .expect("non-empty source should parse");
    assert_eq!(source, "https://example.com/from-name");

    let err = source_from_arg("  ").expect_err("empty source should error");
    assert!(err.to_string().contains("requires a source"));
}

#[tokio::test]
async fn handle_watch_create_writes_only_source_watch_store() -> Result<(), Box<dyn Error>> {
    let tmp = tempfile::tempdir()?;
    let mut cfg = Config::default_minimal();
    cfg.sqlite_path = tmp.path().join("jobs.db");

    handle_watch_create(
        &cfg,
        None,
        "https://example.com/canonical-create".to_string(),
        3600,
        Some("cli-watch-tests".to_string()),
    )
    .await?;

    let store = watch_svc::open_source_watch_store(&cfg, None).await?;
    let page = watch_svc::SourceWatchStoreTrait::list(
        &store,
        watch_svc::WatchListRequest {
            enabled: None,
            source_id: None,
            adapter: None,
            limit: None,
            cursor: None,
        },
    )
    .await?;
    assert_eq!(page.items.len(), 1);
    let found = watch_svc::SourceWatchStoreTrait::get(&store, page.items[0].watch_id.clone())
        .await?
        .expect("canonical watch present");
    assert_eq!(found.canonical_uri, "https://example.com/canonical-create");
    assert_eq!(found.schedule.every_seconds, 3600);
    assert_eq!(found.watch_id, page.items[0].watch_id);
    assert!(found.enabled);
    let stored = store
        .request(found.watch_id.clone())
        .await?
        .expect("stored source watch request");
    assert!(stored.embed, "source watches should embed by default");
    let human = watch_create_human_output(&found);
    assert!(human.contains(&found.watch_id.0));
    assert!(!human.contains("legacy"));
    let json = watch_create_json_output(&found);
    assert_eq!(json["watch_id"], found.watch_id.0);
    assert!(json.get("legacy_watch").is_none());
    assert!(json.get("source_watch").is_none());

    let pool = axon_jobs::store::open_config_pool(&cfg).await?;
    let legacy_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'table' AND name IN ('axon_watch_defs', 'axon_watch_runs')",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        legacy_tables, 0,
        "watch create must not leave retired watch tables in schema"
    );
    Ok(())
}

#[tokio::test]
async fn run_watch_source_shorthand_creates_canonical_watch() -> Result<(), Box<dyn Error>> {
    let tmp = tempfile::tempdir()?;
    let mut cfg = Config::default_minimal();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    cfg.positional = vec![
        "create".to_string(),
        "https://example.com/shorthand/".to_string(),
        "--every-seconds".to_string(),
        "1800".to_string(),
    ];

    let service_context = test_service_context(&cfg).await;
    run_watch(&cfg, &service_context).await?;

    let store = watch_svc::open_source_watch_store(&cfg, None).await?;
    let found = store
        .find_by_source("https://example.com/shorthand")
        .await?
        .expect("source shorthand watch present");
    assert_eq!(found.canonical_uri, "https://example.com/shorthand");
    assert_eq!(found.schedule.every_seconds, 1800);

    cfg.positional = vec![
        "create".to_string(),
        "https://example.com/shorthand".to_string(),
        "--every-seconds".to_string(),
        "3600".to_string(),
    ];
    run_watch(&cfg, &service_context).await?;
    let page = watch_svc::SourceWatchStoreTrait::list(
        &store,
        watch_svc::WatchListRequest {
            enabled: None,
            source_id: None,
            adapter: None,
            limit: None,
            cursor: None,
        },
    )
    .await?;
    assert_eq!(page.items.len(), 1);
    let ensured = watch_svc::SourceWatchStoreTrait::get(&store, found.watch_id)
        .await?
        .expect("ensured source watch present");
    assert_eq!(ensured.schedule.every_seconds, 3600);
    Ok(())
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
async fn handle_watch_exec_records_canonical_history() -> Result<(), Box<dyn Error>> {
    let tmp = tempfile::tempdir()?;
    let mut cfg = Config::default_minimal();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    cfg.json_output = true;
    let service_context = test_service_context(&cfg).await;

    let store = watch_svc::open_source_watch_store(&cfg, None).await?;
    let created = watch_svc::SourceWatchStoreTrait::create(&store, source_watch_request()).await?;
    let id = created.watch_id.0.clone();

    handle_watch_exec(&cfg, &service_context, None, "https://example.com/docs").await?;

    let history = watch_svc::history_source_watch(
        &cfg,
        None,
        watch_svc::WatchHistoryRequest {
            watch_id: watch_svc::WatchId::new(&id),
            limit: Some(10),
            cursor: None,
            status: None,
        },
    )
    .await?;
    assert_eq!(history.watch_id.0, id);
    assert_eq!(history.jobs.len(), 1);
    assert_eq!(history.jobs[0].kind, axon_api::source::JobKind::Source);

    handle_watch_status(&cfg, &service_context, None, "https://example.com/docs").await?;
    handle_watch_history(&cfg, None, "https://example.com/docs", 10).await?;
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
        vec!["status".to_string(), id.clone()],
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
