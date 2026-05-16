use super::*;
use crate::services::context::ServiceContext;
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
        "refresh".to_string(),
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
    let err = handle_watch_create(&cfg, "demo".to_string(), "refresh".to_string(), 0, None)
        .await
        .expect_err("missing interval should error");
    assert!(err.to_string().contains("--every-seconds"));
}

#[tokio::test]
async fn handle_watch_create_rejects_invalid_task_payload_json() {
    let cfg = Config::test_default();
    let err = handle_watch_create(
        &cfg,
        "demo".to_string(),
        "refresh".to_string(),
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
async fn run_watch_lists_with_lite_backend() -> Result<(), Box<dyn Error>> {
    let tmp = tempfile::tempdir()?;
    let mut cfg = Config::default_minimal();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    let service_context = test_service_context(&cfg).await;
    run_watch(&cfg, &service_context).await?;
    Ok(())
}
