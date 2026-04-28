use crate::crates::core::config::Config;
use crate::crates::jobs::lite::query::ms_to_dt;
use crate::crates::jobs::lite::store::{now_ms, open_config_pool};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::error::Error;
use uuid::Uuid;

pub const WATCH_RUN_STATUS_RUNNING: &str = "running";
pub const WATCH_RUN_STATUS_COMPLETED: &str = "completed";
pub const WATCH_RUN_STATUS_FAILED: &str = "failed";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchDef {
    pub id: Uuid,
    pub name: String,
    pub task_type: String,
    pub task_payload: serde_json::Value,
    pub every_seconds: i64,
    pub enabled: bool,
    pub next_run_at: DateTime<Utc>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchDefCreate {
    pub name: String,
    pub task_type: String,
    pub task_payload: serde_json::Value,
    pub every_seconds: i64,
    pub enabled: bool,
    pub next_run_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchRun {
    pub id: Uuid,
    pub watch_id: Uuid,
    pub status: String,
    pub dispatched_job_id: Option<Uuid>,
    pub error_text: Option<String>,
    pub result_json: Option<serde_json::Value>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

type WatchDefRow = (
    String,
    String,
    String,
    String,
    i64,
    i64,
    i64,
    Option<i64>,
    Option<i64>,
    i64,
    i64,
);

type WatchRunRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    i64,
    i64,
);

fn parse_json(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::json!({}))
}

fn parse_watch_def_row(row: WatchDefRow) -> WatchDef {
    let (
        id,
        name,
        task_type,
        task_payload,
        every_seconds,
        enabled,
        next_run_at,
        lease_expires_at,
        last_run_at,
        created_at,
        updated_at,
    ) = row;
    WatchDef {
        id: Uuid::parse_str(&id).unwrap_or_default(),
        name,
        task_type,
        task_payload: parse_json(&task_payload),
        every_seconds,
        enabled: enabled != 0,
        next_run_at: ms_to_dt(next_run_at),
        lease_expires_at: lease_expires_at.map(ms_to_dt),
        last_run_at: last_run_at.map(ms_to_dt),
        created_at: ms_to_dt(created_at),
        updated_at: ms_to_dt(updated_at),
    }
}

fn parse_watch_run_row(row: WatchRunRow) -> WatchRun {
    let (
        id,
        watch_id,
        status,
        dispatched_job_id,
        error_text,
        result_json,
        started_at,
        finished_at,
        created_at,
        updated_at,
    ) = row;
    WatchRun {
        id: Uuid::parse_str(&id).unwrap_or_default(),
        watch_id: Uuid::parse_str(&watch_id).unwrap_or_default(),
        status,
        dispatched_job_id: dispatched_job_id.and_then(|raw| Uuid::parse_str(&raw).ok()),
        error_text,
        result_json: result_json.as_deref().map(parse_json),
        started_at: started_at.map(ms_to_dt),
        finished_at: finished_at.map(ms_to_dt),
        created_at: ms_to_dt(created_at),
        updated_at: ms_to_dt(updated_at),
    }
}

pub async fn create_watch_def(
    cfg: &Config,
    input: &WatchDefCreate,
) -> Result<WatchDef, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    let id = Uuid::new_v4();
    let now = now_ms();
    let row = sqlx::query_as::<_, WatchDefRow>(
        "INSERT INTO axon_watch_defs \
         (id, name, task_type, task_payload, every_seconds, enabled, next_run_at, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
         RETURNING id, name, task_type, task_payload, every_seconds, enabled, next_run_at, lease_expires_at, last_run_at, created_at, updated_at",
    )
    .bind(id.to_string())
    .bind(&input.name)
    .bind(&input.task_type)
    .bind(input.task_payload.to_string())
    .bind(input.every_seconds)
    .bind(if input.enabled { 1 } else { 0 })
    .bind(input.next_run_at.timestamp_millis())
    .bind(now)
    .bind(now)
    .fetch_one(&pool)
    .await?;
    Ok(parse_watch_def_row(row))
}

pub async fn list_watch_defs(cfg: &Config, limit: i64) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    let rows = sqlx::query_as::<_, WatchDefRow>(
        "SELECT id, name, task_type, task_payload, every_seconds, enabled, next_run_at, lease_expires_at, last_run_at, created_at, updated_at \
         FROM axon_watch_defs ORDER BY next_run_at ASC, created_at ASC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&pool)
    .await?;
    Ok(rows.into_iter().map(parse_watch_def_row).collect())
}

pub async fn get_watch_def(
    cfg: &Config,
    watch_id: Uuid,
) -> Result<Option<WatchDef>, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    let row = sqlx::query_as::<_, WatchDefRow>(
        "SELECT id, name, task_type, task_payload, every_seconds, enabled, next_run_at, lease_expires_at, last_run_at, created_at, updated_at \
         FROM axon_watch_defs WHERE id = ?",
    )
    .bind(watch_id.to_string())
    .fetch_optional(&pool)
    .await?;
    Ok(row.map(parse_watch_def_row))
}

pub async fn create_watch_run(
    cfg: &Config,
    watch_id: Uuid,
    dispatched_job_id: Option<Uuid>,
) -> Result<WatchRun, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    create_watch_run_with_pool(&pool, watch_id, dispatched_job_id).await
}

async fn create_watch_run_with_pool(
    pool: &sqlx::SqlitePool,
    watch_id: Uuid,
    dispatched_job_id: Option<Uuid>,
) -> Result<WatchRun, Box<dyn Error>> {
    let id = Uuid::new_v4();
    let now = now_ms();
    let row = sqlx::query_as::<_, WatchRunRow>(
        "INSERT INTO axon_watch_runs \
         (id, watch_id, status, dispatched_job_id, started_at, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         RETURNING id, watch_id, status, dispatched_job_id, error_text, result_json, started_at, finished_at, created_at, updated_at",
    )
    .bind(id.to_string())
    .bind(watch_id.to_string())
    .bind(WATCH_RUN_STATUS_RUNNING)
    .bind(dispatched_job_id.map(|value| value.to_string()))
    .bind(now)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(parse_watch_run_row(row))
}

pub async fn finish_watch_run(
    cfg: &Config,
    watch_id: Uuid,
    run_id: Uuid,
    status: &str,
    result_json: Option<&serde_json::Value>,
    error_text: Option<&str>,
) -> Result<bool, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    finish_watch_run_with_pool(&pool, watch_id, run_id, status, result_json, error_text).await
}

async fn finish_watch_run_with_pool(
    pool: &sqlx::SqlitePool,
    watch_id: Uuid,
    run_id: Uuid,
    status: &str,
    result_json: Option<&serde_json::Value>,
    error_text: Option<&str>,
) -> Result<bool, Box<dyn Error>> {
    let now = now_ms();
    let updated = sqlx::query(
        "UPDATE axon_watch_runs \
         SET status = ?, result_json = ?, error_text = ?, finished_at = ?, updated_at = ? \
         WHERE id = ? AND watch_id = ?",
    )
    .bind(status)
    .bind(result_json.map(serde_json::Value::to_string))
    .bind(error_text)
    .bind(now)
    .bind(now)
    .bind(run_id.to_string())
    .bind(watch_id.to_string())
    .execute(pool)
    .await?
    .rows_affected();

    if updated == 0 {
        return Ok(false);
    }

    sqlx::query(
        "UPDATE axon_watch_defs \
         SET last_run_at = ?, next_run_at = ? + (every_seconds * 1000), lease_expires_at = NULL, updated_at = ? \
         WHERE id = ?",
    )
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(watch_id.to_string())
    .execute(pool)
    .await?;

    Ok(true)
}

pub async fn list_watch_runs(
    cfg: &Config,
    watch_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRun>, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    let rows = sqlx::query_as::<_, WatchRunRow>(
        "SELECT id, watch_id, status, dispatched_job_id, error_text, result_json, started_at, finished_at, created_at, updated_at \
         FROM axon_watch_runs WHERE watch_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(watch_id.to_string())
    .bind(limit)
    .fetch_all(&pool)
    .await?;
    Ok(rows.into_iter().map(parse_watch_run_row).collect())
}

async fn get_watch_run_with_pool(
    pool: &sqlx::SqlitePool,
    run_id: Uuid,
) -> Result<Option<WatchRun>, Box<dyn Error>> {
    let row = sqlx::query_as::<_, WatchRunRow>(
        "SELECT id, watch_id, status, dispatched_job_id, error_text, result_json, started_at, finished_at, created_at, updated_at \
         FROM axon_watch_runs WHERE id = ? LIMIT 1",
    )
    .bind(run_id.to_string())
    .fetch_optional(pool)
    .await?;
    Ok(row.map(parse_watch_run_row))
}

pub async fn run_watch_now(cfg: &Config, watch: &WatchDef) -> Result<WatchRun, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    let run = create_watch_run_with_pool(&pool, watch.id, None).await?;
    let outcome: Result<(), Box<dyn Error>> = match watch.task_type.as_str() {
        "refresh" => {
            let urls = watch
                .task_payload
                .get("urls")
                .and_then(|value| serde_json::from_value::<Vec<String>>(value.clone()).ok())
                .unwrap_or_default();
            if urls.is_empty() {
                Err("watch refresh task requires task_payload.urls".into())
            } else {
                let mut checked = 0usize;
                let mut failed = 0usize;
                let mut refreshed = Vec::new();
                for url in &urls {
                    match crate::crates::services::scrape::scrape(cfg, url, None).await {
                        Ok(result) => {
                            checked += 1;
                            refreshed.push(serde_json::json!({
                                "url": result.url,
                                "markdown_chars": result.markdown.chars().count(),
                            }));
                        }
                        Err(_) => {
                            checked += 1;
                            failed += 1;
                        }
                    }
                }
                let payload = serde_json::json!({
                    "mode": "lite-stateless-refresh",
                    "checked": checked,
                    "changed": 0,
                    "unchanged": checked.saturating_sub(failed),
                    "failed": failed,
                    "urls": urls,
                    "refreshed": refreshed,
                });
                let _ = finish_watch_run_with_pool(
                    &pool,
                    watch.id,
                    run.id,
                    WATCH_RUN_STATUS_COMPLETED,
                    Some(&payload),
                    None,
                )
                .await?;
                Ok(())
            }
        }
        other => Err(format!("unsupported watch task_type in lite mode: {other}").into()),
    };

    if let Err(err) = outcome {
        let _ = finish_watch_run_with_pool(
            &pool,
            watch.id,
            run.id,
            WATCH_RUN_STATUS_FAILED,
            None,
            Some(&err.to_string()),
        )
        .await?;
        return Err(err);
    }

    Ok(get_watch_run_with_pool(&pool, run.id).await?.unwrap_or(run))
}

#[cfg(test)]
mod tests {
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
}
