use anyhow::{Result, bail};
use sqlx::SqlitePool;

use crate::jobs::store::now_ms;

use super::{MAX_LIMIT, MemoryEdgeItem, MemoryItem, NormalizedMemory, edge_id};

#[derive(sqlx::FromRow)]
struct MemoryNodeRow {
    id: String,
    memory_type: String,
    title: String,
    project: Option<String>,
    repo: Option<String>,
    file: Option<String>,
    status: String,
    confidence: f64,
    created_at: i64,
    updated_at: i64,
    last_seen_at: i64,
    access_count: i64,
}

#[derive(sqlx::FromRow)]
struct MemoryEdgeRow {
    id: String,
    source_id: String,
    target_id: String,
    edge_type: String,
    created_at: i64,
    updated_at: i64,
}

impl From<MemoryEdgeRow> for MemoryEdgeItem {
    fn from(row: MemoryEdgeRow) -> Self {
        Self {
            id: row.id,
            source_id: row.source_id,
            target_id: row.target_id,
            edge_type: row.edge_type,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<MemoryNodeRow> for MemoryItem {
    fn from(row: MemoryNodeRow) -> Self {
        Self {
            id: row.id,
            memory_type: row.memory_type,
            title: row.title,
            body: None,
            project: row.project,
            repo: row.repo,
            file: row.file,
            confidence: row.confidence,
            status: row.status,
            created_at: row.created_at,
            updated_at: row.updated_at,
            last_seen_at: row.last_seen_at,
            access_count: row.access_count,
            score: None,
        }
    }
}

pub(super) async fn upsert_node(
    pool: &SqlitePool,
    memory: &NormalizedMemory,
    now: i64,
) -> Result<()> {
    let mut conn = pool.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let result = sqlx::query(
        r#"
        INSERT INTO axon_memory_nodes
            (id, type, title, project, repo, file_path, status, confidence, source,
             access_count, created_at, updated_at, last_seen_at)
        VALUES (?, ?, ?, ?, ?, ?, 'active', ?, 'manual', 0, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            type=excluded.type,
            title=excluded.title,
            project=excluded.project,
            repo=excluded.repo,
            file_path=excluded.file_path,
            status='active',
            confidence=excluded.confidence,
            updated_at=excluded.updated_at,
            last_seen_at=excluded.last_seen_at
        "#,
    )
    .bind(memory.id.to_string())
    .bind(&memory.memory_type)
    .bind(&memory.title)
    .bind(&memory.project)
    .bind(&memory.repo)
    .bind(&memory.file)
    .bind(memory.confidence)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&mut *conn)
    .await;
    match result {
        Ok(_) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(())
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err.into())
        }
    }
}

pub(super) async fn node_by_id(pool: &SqlitePool, id: &str) -> Result<MemoryItem> {
    node_by_id_optional(pool, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("memory not found: {id}"))
}

pub(super) async fn node_by_id_optional(pool: &SqlitePool, id: &str) -> Result<Option<MemoryItem>> {
    let row = sqlx::query_as::<_, MemoryNodeRow>(
        r#"
        SELECT id, type AS memory_type, title, project, repo, file_path AS file,
               status, confidence, created_at, updated_at, last_seen_at, access_count
        FROM axon_memory_nodes
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

pub(super) async fn list_nodes(
    pool: &SqlitePool,
    project: Option<&str>,
    repo: Option<&str>,
    file: Option<&str>,
    memory_type: Option<&str>,
    status: Option<&str>,
    limit: usize,
) -> Result<Vec<MemoryItem>> {
    let limit = limit.clamp(1, MAX_LIMIT);
    let status = normalize_status_filter(status)?;
    let rows = sqlx::query_as::<_, MemoryNodeRow>(
        r#"
        SELECT id, type AS memory_type, title, project, repo, file_path AS file,
               status, confidence, created_at, updated_at, last_seen_at, access_count
        FROM axon_memory_nodes
        WHERE status = ?
          AND (? IS NULL OR project = ?)
          AND (? IS NULL OR repo = ?)
          AND (? IS NULL OR file_path = ?)
          AND (? IS NULL OR type = ?)
        ORDER BY updated_at DESC, created_at DESC, id ASC
        LIMIT ?
        "#,
    )
    .bind(status)
    .bind(clean_filter_ref(project))
    .bind(clean_filter_ref(project))
    .bind(clean_filter_ref(repo))
    .bind(clean_filter_ref(repo))
    .bind(clean_filter_ref(file))
    .bind(clean_filter_ref(file))
    .bind(clean_filter_ref(memory_type))
    .bind(clean_filter_ref(memory_type))
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

fn normalize_status_filter(status: Option<&str>) -> Result<&'static str> {
    match status.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("active") => Ok("active"),
        Some("superseded") => Ok("superseded"),
        Some("archived") => Ok("archived"),
        Some(other) => bail!("unknown memory status: {other}"),
    }
}

fn clean_filter_ref(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub(super) async fn context_seed_nodes(
    pool: &SqlitePool,
    project: Option<&str>,
    repo: Option<&str>,
    file: Option<&str>,
    seed_ids: &[String],
    limit: usize,
) -> Result<Vec<MemoryItem>> {
    let limit = limit.clamp(1, MAX_LIMIT);
    let mut items = Vec::new();
    if seed_ids.is_empty() {
        let rows = sqlx::query_as::<_, MemoryNodeRow>(
            r#"
            SELECT id, type AS memory_type, title, project, repo, file_path AS file,
                   status, confidence, created_at, updated_at, last_seen_at, access_count
            FROM axon_memory_nodes
            WHERE status = 'active'
              AND (? IS NULL OR project = ?)
              AND (? IS NULL OR repo = ?)
              AND (? IS NULL OR file_path = ?)
            ORDER BY updated_at DESC, created_at DESC, id ASC
            LIMIT ?
            "#,
        )
        .bind(project)
        .bind(project)
        .bind(repo)
        .bind(repo)
        .bind(file)
        .bind(file)
        .bind(limit as i64)
        .fetch_all(pool)
        .await?;
        items.extend(rows.into_iter().map(MemoryItem::from));
    } else {
        for id in seed_ids.iter().take(limit) {
            if let Some(item) = node_by_id_optional(pool, id).await?
                && item.status == "active"
            {
                push_unique_memory(&mut items, item);
            }
        }
    }

    let seeds = items.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
    for seed_id in seeds {
        if items.len() >= limit {
            break;
        }
        let rows = sqlx::query_as::<_, MemoryNodeRow>(
            r#"
            SELECT n.id, n.type AS memory_type, n.title, n.project, n.repo, n.file_path AS file,
                   n.status, n.confidence, n.created_at, n.updated_at, n.last_seen_at, n.access_count
            FROM axon_memory_edges e
            JOIN axon_memory_nodes n
              ON n.id = CASE
                WHEN e.source_id = ? THEN e.target_id
                ELSE e.source_id
              END
            WHERE (e.source_id = ? OR e.target_id = ?)
              AND n.status = 'active'
            ORDER BY e.updated_at DESC, n.updated_at DESC, n.id ASC
            "#,
        )
        .bind(&seed_id)
        .bind(&seed_id)
        .bind(&seed_id)
        .fetch_all(pool)
        .await?;
        for row in rows {
            if items.len() >= limit {
                break;
            }
            push_unique_memory(&mut items, row.into());
        }
    }
    Ok(items)
}

fn push_unique_memory(items: &mut Vec<MemoryItem>, item: MemoryItem) {
    if !items.iter().any(|existing| existing.id == item.id) {
        items.push(item);
    }
}

pub(super) async fn link_nodes(
    pool: &SqlitePool,
    source_id: &str,
    target_id: &str,
    edge_type: &str,
    now: i64,
) -> Result<MemoryEdgeItem> {
    if source_id == target_id {
        bail!("source_id and target_id must be different");
    }
    if node_by_id_optional(pool, source_id).await?.is_none() {
        bail!("memory not found: {source_id}");
    }
    if node_by_id_optional(pool, target_id).await?.is_none() {
        bail!("memory not found: {target_id}");
    }

    let id = edge_id(source_id, target_id, edge_type).to_string();
    let mut conn = pool.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let result = sqlx::query(
        r#"
        INSERT INTO axon_memory_edges
            (id, source_id, target_id, type, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(source_id, target_id, type) DO UPDATE SET
            updated_at=excluded.updated_at
        "#,
    )
    .bind(&id)
    .bind(source_id)
    .bind(target_id)
    .bind(edge_type)
    .bind(now)
    .bind(now)
    .execute(&mut *conn)
    .await;
    match result {
        Ok(_) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            edge_by_id(pool, &id).await
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err.into())
        }
    }
}

pub(super) async fn supersede_node(
    pool: &SqlitePool,
    replacement_id: &str,
    superseded_id: &str,
    now: i64,
) -> Result<MemoryEdgeItem> {
    if replacement_id == superseded_id {
        bail!("source_id and target_id must be different");
    }
    if node_by_id_optional(pool, replacement_id).await?.is_none() {
        bail!("memory not found: {replacement_id}");
    }
    if node_by_id_optional(pool, superseded_id).await?.is_none() {
        bail!("memory not found: {superseded_id}");
    }

    let id = edge_id(replacement_id, superseded_id, "supersedes").to_string();
    let mut conn = pool.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let result = async {
        sqlx::query(
            r#"
            UPDATE axon_memory_nodes
            SET status = 'superseded', updated_at = ?, last_seen_at = ?
            WHERE id = ?
            "#,
        )
        .bind(now)
        .bind(now)
        .bind(superseded_id)
        .execute(&mut *conn)
        .await?;
        sqlx::query(
            r#"
            INSERT INTO axon_memory_edges
                (id, source_id, target_id, type, created_at, updated_at)
            VALUES (?, ?, ?, 'supersedes', ?, ?)
            ON CONFLICT(source_id, target_id, type) DO UPDATE SET
                updated_at=excluded.updated_at
            "#,
        )
        .bind(&id)
        .bind(replacement_id)
        .bind(superseded_id)
        .bind(now)
        .bind(now)
        .execute(&mut *conn)
        .await?;
        Ok::<_, sqlx::Error>(())
    }
    .await;
    match result {
        Ok(()) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            edge_by_id(pool, &id).await
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err.into())
        }
    }
}

async fn edge_by_id(pool: &SqlitePool, id: &str) -> Result<MemoryEdgeItem> {
    sqlx::query_as::<_, MemoryEdgeRow>(
        r#"
        SELECT id, source_id, target_id, type AS edge_type, created_at, updated_at
        FROM axon_memory_edges
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map(Into::into)
    .map_err(Into::into)
}

pub(super) async fn bump_access(pool: &SqlitePool, ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let now = now_ms();
    let mut conn = pool.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    for id in ids {
        if let Err(err) = sqlx::query(
            r#"
            UPDATE axon_memory_nodes
            SET access_count = access_count + 1, last_seen_at = ?
            WHERE id = ?
            "#,
        )
        .bind(now)
        .bind(id)
        .execute(&mut *conn)
        .await
        {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(err.into());
        }
    }
    sqlx::query("COMMIT").execute(&mut *conn).await?;
    Ok(())
}
