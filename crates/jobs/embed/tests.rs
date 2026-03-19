use super::worker::process_embed_job_with_runner;
use super::*;
use crate::crates::jobs::common::{open_amqp_channel, resolve_test_pg_url, test_config};
use chrono::{Duration, Utc};
use serial_test::serial;
use std::sync::{Arc, Mutex};
use tokio::time::{Duration as TokioDuration, sleep, timeout};

#[test]
fn embed_job_config_includes_source_type() {
    let cfg = EmbedJobConfig {
        collection: "test".into(),
        source_type: Some("crawl".into()),
    };
    let json = serde_json::to_value(&cfg).unwrap();
    assert_eq!(json["source_type"], "crawl");
}

#[test]
fn embed_job_config_deserializes_without_source_type() {
    let json = r#"{"collection":"cortex"}"#;
    let cfg: EmbedJobConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.source_type, None);
}

fn amqp_url() -> Option<String> {
    // Do not fall through to AXON_AMQP_URL — that is the production broker.
    // If AXON_TEST_AMQP_URL is not set, AMQP tests are skipped.
    std::env::var("AXON_TEST_AMQP_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
}

#[tokio::test]
#[serial]
async fn embed_start_job_dedupes_active_pending_job() -> Result<(), Box<dyn Error>> {
    let Some(pg_url) = resolve_test_pg_url() else {
        return Ok(());
    };
    let cfg = test_config(&pg_url);
    if make_pool(&cfg).await.is_err() {
        return Ok(());
    }
    let input = format!("embed-dedupe-{}", Uuid::new_v4());

    let first_id = start_embed_job(&cfg, &input, None).await?;
    let second_id = start_embed_job(&cfg, &input, None).await?;
    assert_eq!(first_id, second_id);

    let pool = match make_pool(&cfg).await {
        Ok(pool) => pool,
        Err(_) => return Ok(()),
    };
    let _ = sqlx::query("DELETE FROM axon_embed_jobs WHERE id = $1")
        .bind(first_id)
        .execute(&pool)
        .await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn embed_start_job_dedupes_fresh_running_job() -> Result<(), Box<dyn Error>> {
    let Some(pg_url) = resolve_test_pg_url() else {
        return Ok(());
    };
    let mut cfg = test_config(&pg_url);
    if make_pool(&cfg).await.is_err() {
        return Ok(());
    }
    cfg.watchdog_stale_timeout_secs = 300;
    let input = format!("embed-running-dedupe-fresh-{}", Uuid::new_v4());

    let first_id = start_embed_job(&cfg, &input, None).await?;
    let pool = match make_pool(&cfg).await {
        Ok(pool) => pool,
        Err(_) => return Ok(()),
    };
    sqlx::query("UPDATE axon_embed_jobs SET status='running', updated_at=NOW() WHERE id=$1")
        .bind(first_id)
        .execute(&pool)
        .await?;

    let second_id = start_embed_job(&cfg, &input, None).await?;
    assert_eq!(first_id, second_id);

    let _ = sqlx::query("DELETE FROM axon_embed_jobs WHERE id = $1")
        .bind(first_id)
        .execute(&pool)
        .await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn embed_start_job_creates_new_when_running_job_is_stale() -> Result<(), Box<dyn Error>> {
    let Some(pg_url) = resolve_test_pg_url() else {
        return Ok(());
    };
    let mut cfg = test_config(&pg_url);
    if make_pool(&cfg).await.is_err() {
        return Ok(());
    }
    cfg.watchdog_stale_timeout_secs = 30;
    let input = format!("embed-running-dedupe-stale-{}", Uuid::new_v4());

    let first_id = start_embed_job(&cfg, &input, None).await?;
    let pool = match make_pool(&cfg).await {
        Ok(pool) => pool,
        Err(_) => return Ok(()),
    };
    sqlx::query(
        "UPDATE axon_embed_jobs SET status='running', updated_at=NOW() - INTERVAL '2 minutes' WHERE id=$1",
    )
    .bind(first_id)
    .execute(&pool)
    .await?;

    let second_id = start_embed_job(&cfg, &input, None).await?;
    assert_ne!(first_id, second_id);

    let _ = sqlx::query("DELETE FROM axon_embed_jobs WHERE id = $1 OR id = $2")
        .bind(first_id)
        .bind(second_id)
        .execute(&pool)
        .await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn embed_recover_reclaims_confirmed_stale_running_job() -> Result<(), Box<dyn Error>> {
    let Some(pg_url) = resolve_test_pg_url() else {
        return Ok(());
    };
    let cfg = test_config(&pg_url);
    if make_pool(&cfg).await.is_err() {
        return Ok(());
    }
    let input = format!("embed-recover-{}", Uuid::new_v4());
    let id = start_embed_job(&cfg, &input, None).await?;
    let pool = match make_pool(&cfg).await {
        Ok(pool) => pool,
        Err(_) => return Ok(()),
    };

    sqlx::query(
        "UPDATE axon_embed_jobs SET status='running', updated_at=NOW() - INTERVAL '20 minutes' WHERE id=$1",
    )
    .bind(id)
    .execute(&pool)
    .await?;

    let observed_updated_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM axon_embed_jobs WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await?;
    let watchdog = serde_json::json!({
        "_watchdog": {
            "observed_updated_at": observed_updated_at.to_rfc3339(),
            "first_seen_stale_at": (Utc::now() - Duration::minutes(10)).to_rfc3339()
        }
    });
    sqlx::query("UPDATE axon_embed_jobs SET result_json=$2 WHERE id=$1")
        .bind(id)
        .bind(watchdog)
        .execute(&pool)
        .await?;

    let reclaimed = recover_stale_embed_jobs(&cfg).await?;
    assert!(reclaimed >= 1);

    let status: String = sqlx::query_scalar("SELECT status FROM axon_embed_jobs WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(status, "failed");

    let _ = sqlx::query("DELETE FROM axon_embed_jobs WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn embed_ensure_schema_is_concurrency_safe() -> Result<(), Box<dyn Error>> {
    let Some(pg_url) = resolve_test_pg_url() else {
        return Ok(());
    };
    let cfg = test_config(&pg_url);
    if make_pool(&cfg).await.is_err() {
        return Ok(());
    }
    let pool = match make_pool(&cfg).await {
        Ok(pool) => pool,
        Err(_) => return Ok(()),
    };
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let pool = pool.clone();
        tasks.push(tokio::spawn(async move { ensure_schema(&pool).await }));
    }
    for task in tasks {
        let result = task.await?;
        result?;
    }
    Ok(())
}

#[tokio::test]
#[serial]
async fn embed_worker_uses_source_type_from_job_config() -> Result<(), Box<dyn Error>> {
    let Some(pg_url) = resolve_test_pg_url() else {
        return Ok(());
    };
    let cfg = test_config(&pg_url);
    if make_pool(&cfg).await.is_err() {
        return Ok(());
    }
    let pool = match make_pool(&cfg).await {
        Ok(pool) => pool,
        Err(_) => return Ok(()),
    };

    let input = format!("embed-worker-source-type-{}", Uuid::new_v4());
    let id = start_embed_job(&cfg, &input, Some("crawl")).await?;
    sqlx::query("UPDATE axon_embed_jobs SET status='running', updated_at=NOW() WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await?;

    let seen_source_type = Arc::new(Mutex::new(None::<String>));
    let seen_source_type_cloned = Arc::clone(&seen_source_type);
    let result = process_embed_job_with_runner(
        &cfg,
        &pool,
        id,
        move |_cfg, _pool, _id, _input_text, collection, source_type| {
            let source_type = source_type.map(str::to_string);
            let seen_source_type_cloned = Arc::clone(&seen_source_type_cloned);
            Box::pin(async move {
                *seen_source_type_cloned.lock().expect("lock") = source_type;
                Ok(serde_json::json!({
                    "input": input,
                    "collection": collection,
                    "docs_embedded": 1usize,
                    "chunks_embedded": 1usize,
                    "source": "mock",
                }))
            })
        },
    )
    .await;

    assert!(result.is_ok(), "worker helper should complete successfully");
    assert_eq!(
        seen_source_type.lock().expect("lock").as_deref(),
        Some("crawl")
    );

    let status: String = sqlx::query_scalar("SELECT status FROM axon_embed_jobs WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(status, "completed");

    let config_json: serde_json::Value =
        sqlx::query_scalar("SELECT config_json FROM axon_embed_jobs WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(config_json["source_type"], "crawl");

    let _ = sqlx::query("DELETE FROM axon_embed_jobs WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires AMQP infra"]
async fn embed_worker_e2e_processes_pending_job_to_terminal_status() -> Result<(), Box<dyn Error>> {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let Some(pg_url) = resolve_test_pg_url() else {
                return Ok(());
            };
            let Some(_) = amqp_url() else {
                return Ok(());
            };
            let cfg = test_config(&pg_url);
            if make_pool(&cfg).await.is_err() {
                return Ok(());
            }
            if open_amqp_channel(&cfg, &cfg.embed_queue).await.is_err() {
                return Ok(());
            }
            let input = format!("embed-worker-e2e-{}", Uuid::new_v4());
            let id = start_embed_job(&cfg, &input, None).await?;

            let worker_cfg = cfg.clone();
            let worker = tokio::task::spawn_local(async move {
                let _ = run_embed_worker(&worker_cfg).await;
            });

            let pool = match make_pool(&cfg).await {
                Ok(pool) => pool,
                Err(_) => return Ok(()),
            };
            let wait = timeout(TokioDuration::from_secs(8), async {
                loop {
                    let status: Option<String> =
                        sqlx::query_scalar("SELECT status FROM axon_embed_jobs WHERE id=$1")
                            .bind(id)
                            .fetch_optional(&pool)
                            .await
                            .ok()
                            .flatten();
                    if matches!(status.as_deref(), Some("completed" | "failed" | "canceled")) {
                        break;
                    }
                    sleep(TokioDuration::from_millis(100)).await;
                }
            })
            .await;
            worker.abort();
            let _ = worker.await;
            assert!(
                wait.is_ok(),
                "embed worker did not reach terminal state in time"
            );

            let status: String =
                sqlx::query_scalar("SELECT status FROM axon_embed_jobs WHERE id = $1")
                    .bind(id)
                    .fetch_one(&pool)
                    .await?;
            assert!(matches!(
                status.as_str(),
                "completed" | "failed" | "canceled"
            ));

            let _ = sqlx::query("DELETE FROM axon_embed_jobs WHERE id = $1")
                .bind(id)
                .execute(&pool)
                .await;
            Ok::<(), Box<dyn Error>>(())
        })
        .await
}
