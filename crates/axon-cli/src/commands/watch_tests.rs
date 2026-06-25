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
    let err = parse_uuid(Some(&raw), "run-now").expect_err("invalid uuid should error");
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
