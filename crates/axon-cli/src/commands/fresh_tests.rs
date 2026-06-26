use super::*;
use axon_core::config::{CommandKind, Config, FreshnessCommand, FreshnessRequest};
use axon_services::context::ServiceContext;
use std::sync::Arc;

fn freshness_cfg(command: CommandKind, target: &str, fresh_command: FreshnessCommand) -> Config {
    let temp = tempfile::tempdir().expect("tempdir");
    let sqlite_path = temp.path().join("jobs.db");
    // Keep the path alive after the helper returns; SQLite creates the file lazily.
    std::mem::forget(temp);
    let mut cfg = Config::test_default();
    cfg.sqlite_path = sqlite_path;
    cfg.command = command;
    cfg.positional = vec![target.to_string()];
    cfg.freshness = Some(FreshnessRequest {
        command: fresh_command,
        every_seconds: 86_400,
    });
    cfg.json_output = true;
    cfg
}

async fn context(cfg: &Config) -> ServiceContext {
    ServiceContext::new(Arc::new(cfg.clone()))
        .await
        .expect("service context")
}

#[tokio::test]
async fn create_scrape_schedule_writes_db() {
    let cfg = freshness_cfg(
        CommandKind::Scrape,
        "https://example.com",
        FreshnessCommand::Scrape,
    );
    let ctx = context(&cfg).await;
    create_schedule_from_command(&cfg, &ctx)
        .await
        .expect("create schedule");
    let schedules = axon_services::freshness::list(&ctx, 10)
        .await
        .expect("list");
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].command, "scrape");
    assert_eq!(schedules[0].target, "https://example.com");
}

#[tokio::test]
async fn create_crawl_embed_and_ingest_schedules() {
    for (command, target, fresh_command, stored_command) in [
        (
            CommandKind::Crawl,
            "https://example.com/docs",
            FreshnessCommand::Crawl,
            "crawl",
        ),
        (
            CommandKind::Embed,
            "fresh text",
            FreshnessCommand::Embed,
            "embed",
        ),
        (
            CommandKind::Ingest,
            "rss:https://example.com/feed.xml",
            FreshnessCommand::Ingest,
            "ingest",
        ),
    ] {
        let cfg = freshness_cfg(command, target, fresh_command);
        let ctx = context(&cfg).await;
        create_schedule_from_command(&cfg, &ctx)
            .await
            .expect("create schedule");
        let schedules = axon_services::freshness::list(&ctx, 10)
            .await
            .expect("list");
        assert_eq!(schedules.len(), 1);
        assert_eq!(schedules[0].command, stored_command);
    }
}

#[tokio::test]
async fn fresh_list_run_now_and_history_execute() {
    let mut cfg = freshness_cfg(CommandKind::Embed, "fresh text", FreshnessCommand::Embed);
    let ctx = context(&cfg).await;
    let created = axon_services::freshness::create_from_config(&cfg, &ctx)
        .await
        .expect("create");

    cfg.command = CommandKind::Fresh;
    cfg.freshness = None;
    cfg.positional = vec!["list".to_string(), "--json".to_string()];
    run_fresh(&cfg, &ctx).await.expect("list");

    cfg.positional = vec![
        "run-now".to_string(),
        created.id.to_string(),
        "--json".to_string(),
    ];
    run_fresh(&cfg, &ctx).await.expect("run-now");

    cfg.positional = vec![
        "history".to_string(),
        created.id.to_string(),
        "--limit".to_string(),
        "50".to_string(),
        "--json".to_string(),
    ];
    run_fresh(&cfg, &ctx).await.expect("history");
    let runs = axon_services::freshness::history(&ctx, created.id, 50)
        .await
        .expect("history rows");
    assert_eq!(runs.len(), 1);
    assert!(runs[0].dispatched_job_id.is_some());
}
