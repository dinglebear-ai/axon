use crate::freshness::{
    FRESHNESS_RUN_STATUS_COMPLETED, FreshnessDefCreate, create_freshness_def_with_pool,
    create_freshness_run_with_pool, finish_freshness_run_with_pool, get_freshness_def_with_pool,
    heartbeat_freshness_run, lease_due_freshness, list_freshness_defs_with_pool,
    list_freshness_runs_with_pool, reclaim_stale_freshness_leases, stable_initial_jitter_seconds,
};
use crate::store::now_ms;
use chrono::{Duration, TimeZone, Utc};
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::error::Error;
use tempfile::NamedTempFile;
use uuid::Uuid;

async fn test_pool() -> Result<SqlitePool, Box<dyn Error>> {
    let temp = NamedTempFile::new()?;
    let path = temp.into_temp_path().keep()?;
    Ok(crate::store::open_sqlite_pool(&path.to_string_lossy()).await?)
}

fn at_ms(ms: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().expect("valid ms")
}

fn freshness_input(
    identity_hash: &str,
    command: &str,
    target: &str,
    collection: &str,
    next_run_at: chrono::DateTime<Utc>,
) -> FreshnessDefCreate {
    FreshnessDefCreate {
        name: format!("{command}-{collection}"),
        command: command.to_string(),
        target: target.to_string(),
        identity_hash: identity_hash.to_string(),
        request_json: serde_json::json!({
            "schema_version": "v1",
            "command": command,
            "url": target
        }),
        config_json: serde_json::json!({
            "collection": collection,
            "render_mode": "http"
        }),
        every_seconds: 86_400,
        enabled: true,
        next_run_at: Some(next_run_at),
    }
}

async fn insert_freshness(
    pool: &SqlitePool,
    identity_hash: &str,
    command: &str,
    target: &str,
    collection: &str,
) -> Result<(), Box<dyn Error>> {
    create_freshness_def_with_pool(
        pool,
        &freshness_input(
            identity_hash,
            command,
            target,
            collection,
            Utc::now() + Duration::days(1),
        ),
    )
    .await?;
    Ok(())
}

async fn insert_due_freshness(
    pool: &SqlitePool,
    identity_hash: &str,
    command: &str,
    target: &str,
) -> Result<Uuid, Box<dyn Error>> {
    let created = create_freshness_def_with_pool(
        pool,
        &freshness_input(
            identity_hash,
            command,
            target,
            "axon-test",
            Utc::now() - Duration::seconds(10),
        ),
    )
    .await?;
    Ok(created.id)
}

#[tokio::test]
async fn create_and_list_freshness_def_round_trips_safe_payload() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let input = FreshnessDefCreate {
        name: "daily-mcp-spec".to_string(),
        command: "scrape".to_string(),
        target: "https://modelcontextprotocol.io/specification".to_string(),
        identity_hash: "hash-a".to_string(),
        request_json: serde_json::json!({
            "schema_version": "v1",
            "command": "scrape",
            "url": "https://modelcontextprotocol.io/specification"
        }),
        config_json: serde_json::json!({
            "collection": "axon-test",
            "render_mode": "http"
        }),
        every_seconds: 86_400,
        enabled: true,
        next_run_at: Some(Utc::now() + Duration::days(1)),
    };
    let created = create_freshness_def_with_pool(&pool, &input).await?;
    let listed = list_freshness_defs_with_pool(&pool, 10).await?;
    assert_eq!(listed[0].id, created.id);
    assert_eq!(listed[0].identity_hash, "hash-a");
    assert_eq!(listed[0].request_json["command"], "scrape");
    Ok(())
}

#[tokio::test]
async fn identity_hash_allows_same_target_in_different_collections() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    insert_freshness(&pool, "hash-prod", "scrape", "https://example.com", "prod").await?;
    insert_freshness(&pool, "hash-test", "scrape", "https://example.com", "test").await?;
    assert_eq!(list_freshness_defs_with_pool(&pool, 10).await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn lease_due_freshness_is_single_flight_and_advances_next_run() -> Result<(), Box<dyn Error>>
{
    let pool = test_pool().await?;
    let id = insert_due_freshness(
        &pool,
        "hash-a",
        "ingest",
        "rss:https://example.com/feed.xml",
    )
    .await?;
    let now = now_ms();
    let first = lease_due_freshness(&pool, now, 300_000, 4).await?;
    let second = lease_due_freshness(&pool, now, 300_000, 4).await?;
    assert_eq!(first[0].id, id);
    assert!(second.is_empty());
    assert!(first[0].next_run_at.timestamp_millis() > now);
    Ok(())
}

#[tokio::test]
async fn lease_due_freshness_honors_configured_limit_above_default() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    for index in 0..6 {
        insert_due_freshness(
            &pool,
            &format!("hash-{index}"),
            "ingest",
            &format!("rss:https://example.com/feed-{index}.xml"),
        )
        .await?;
    }

    let leased = lease_due_freshness(&pool, now_ms(), 300_000, 6).await?;
    assert_eq!(leased.len(), 6);
    Ok(())
}

#[tokio::test]
async fn creation_without_next_run_uses_stable_jitter() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let before = now_ms();
    let created = create_freshness_def_with_pool(
        &pool,
        &FreshnessDefCreate {
            name: "jittered".to_string(),
            command: "scrape".to_string(),
            target: "https://example.com".to_string(),
            identity_hash: "0123456789abcdef".to_string(),
            request_json: serde_json::json!({"schema_version":"v1","command":"scrape","url":"https://example.com"}),
            config_json: serde_json::json!({"collection":"axon-test"}),
            every_seconds: 86_400,
            enabled: true,
            next_run_at: None,
        },
    )
    .await?;
    let expected_jitter = stable_initial_jitter_seconds(&created.identity_hash, 86_400);
    assert!(created.next_run_at >= at_ms(before + 86_400_000 + expected_jitter * 1_000));
    Ok(())
}

#[test]
fn stable_jitter_spreads_same_interval_identities() {
    let mut offsets = HashSet::new();
    for i in 0..100 {
        let identity = format!("{i:016x}");
        offsets.insert(stable_initial_jitter_seconds(&identity, 86_400));
    }
    assert!(offsets.len() > 1, "identities should not share one offset");
}

#[tokio::test]
async fn finish_redacts_and_caps_run_payloads() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let id = insert_due_freshness(&pool, "hash-a", "scrape", "https://example.com").await?;
    let run = create_freshness_run_with_pool(&pool, id, None).await?;
    let long_secret = format!("Authorization:Bearer sk-secret {}", "x".repeat(100_000));
    finish_freshness_run_with_pool(
        &pool,
        id,
        run.id,
        FRESHNESS_RUN_STATUS_COMPLETED,
        Some(&serde_json::json!({"token": "ghp_supersecret", "body": "ok"})),
        Some(&long_secret),
    )
    .await?;
    let runs = list_freshness_runs_with_pool(&pool, id, 10).await?;
    let error = runs[0].error_text.as_deref().expect("error text");
    assert!(!error.contains("sk-secret"));
    assert!(error.len() <= 4096);
    assert!(
        !runs[0]
            .result_json
            .as_ref()
            .expect("result json")
            .to_string()
            .contains("ghp_supersecret")
    );
    Ok(())
}

#[tokio::test]
async fn heartbeat_extends_active_lease() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let id = insert_due_freshness(&pool, "hash-a", "scrape", "https://example.com").await?;
    let now = now_ms();
    let leased = lease_due_freshness(&pool, now, 1_000, 4).await?;
    let run = create_freshness_run_with_pool(&pool, id, None).await?;
    assert_eq!(leased.len(), 1);
    heartbeat_freshness_run(&pool, id, run.id, now + 30_000).await?;
    let stored = get_freshness_def_with_pool(&pool, id)
        .await?
        .expect("stored freshness");
    assert!(stored.lease_expires_at.expect("lease").timestamp_millis() >= now + 30_000);
    Ok(())
}

#[tokio::test]
async fn stale_freshness_leases_are_reclaimed() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let id = insert_due_freshness(&pool, "hash-a", "scrape", "https://example.com").await?;
    let now = now_ms();
    lease_due_freshness(&pool, now, 1, 4).await?;
    let reclaimed = reclaim_stale_freshness_leases(&pool, now + 10).await?;
    assert_eq!(reclaimed, 1);
    let stored = get_freshness_def_with_pool(&pool, id)
        .await?
        .expect("stored freshness");
    assert!(stored.lease_expires_at.is_none());
    Ok(())
}
