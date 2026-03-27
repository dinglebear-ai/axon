//! Graph extraction job persistence and schema entry points.

use crate::crates::core::config::Config;
use crate::crates::jobs::common::{enqueue_job, make_pool, sort_rows_for_status_view};
use crate::crates::jobs::status::JobStatus;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use sqlx::PgPool;

pub(crate) mod context;
pub(crate) mod extract;
mod schema;
pub(crate) mod similarity;
pub(crate) mod taxonomy;
pub(crate) mod worker;

pub use schema::{ensure_graph_schema, ensure_neo4j_schema};
pub use worker::run_graph_worker;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct GraphJob {
    pub id: uuid::Uuid,
    pub url: String,
    pub status: String,
    pub chunk_count: i32,
    pub entity_count: i32,
    pub relation_count: i32,
    pub error_text: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

pub async fn enqueue_graph_job(
    pool: &PgPool,
    cfg: &Config,
    url: &str,
    source_type: &str,
) -> Result<uuid::Uuid, Box<dyn std::error::Error>> {
    ensure_graph_schema(pool).await?;

    let mut tx = pool.begin().await?;
    // Obtain an advisory lock based on the URL to prevent concurrent enqueue races
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("graph_job:{}", url))
        .execute(&mut *tx)
        .await?;

    let active = sqlx::query_scalar::<_, uuid::Uuid>(
        r#"
        SELECT id
        FROM axon_graph_jobs
        WHERE url = $1
          AND status IN ($2, $3)
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(url)
    .bind(JobStatus::Pending.as_str())
    .bind(JobStatus::Running.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(id) = active {
        tx.commit().await?;
        return Ok(id);
    }

    let id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO axon_graph_jobs (id, url, status, config_json)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(id)
    .bind(url)
    .bind(JobStatus::Pending.as_str())
    .bind(serde_json::json!({ "source_type": source_type }))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    if let Err(err) = enqueue_job(cfg, &cfg.graph_queue, id).await {
        crate::crates::jobs::common::mark_job_failed(
            pool,
            crate::crates::jobs::common::JobTable::Graph,
            id,
            &err.to_string(),
        )
        .await?;
        return Err(format!("graph enqueue failed for {id}: {err}").into());
    }

    Ok(id)
}

pub async fn get_graph_job(
    cfg: &Config,
    id: uuid::Uuid,
) -> Result<Option<GraphJob>, Box<dyn std::error::Error>> {
    let pool = make_pool(cfg).await?;
    ensure_graph_schema(&pool).await?;
    let row = sqlx::query_as::<_, GraphJob>(
        r#"
        SELECT id, url, status, chunk_count, entity_count, relation_count, error_text,
               created_at, updated_at, started_at, finished_at
        FROM axon_graph_jobs
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?;
    Ok(row)
}

pub async fn list_graph_jobs(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<Vec<GraphJob>, Box<dyn std::error::Error>> {
    let pool = make_pool(cfg).await?;
    ensure_graph_schema(&pool).await?;
    let mut rows = sqlx::query_as::<_, GraphJob>(
        r#"
        SELECT id, url, status, chunk_count, entity_count, relation_count, error_text,
               created_at, updated_at, started_at, finished_at
        FROM axon_graph_jobs
        ORDER BY
            CASE status
                WHEN 'running' THEN 0
                WHEN 'pending' THEN 1
                WHEN 'completed' THEN 2
                WHEN 'failed' THEN 3
                WHEN 'canceled' THEN 4
                ELSE 5
            END,
            created_at DESC,
            updated_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;
    sort_rows_for_status_view(
        &mut rows,
        |job| job.status.as_str(),
        |job| &job.created_at,
        |job| &job.updated_at,
    );
    Ok(rows)
}

#[cfg(test)]
mod tests {
    #[test]
    fn graph_job_table_name() {
        assert_eq!(
            crate::crates::jobs::common::JobTable::Graph.as_str(),
            "axon_graph_jobs"
        );
    }
}
