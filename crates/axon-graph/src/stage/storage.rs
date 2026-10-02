use super::*;
use futures_util::TryStreamExt;
use sqlx::Row;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

// Allow SQLite canonical rows, indexes, seed data, and journals their own
// bounded overhead without reducing the logical generation payload budget.
const PHYSICAL_STAGE_LIMIT: u64 = 4 * LIMIT;
pub(super) fn error(e: sqlx::Error) -> axon_api::source::ApiError {
    graph_storage_error(format!(
        "graph stage SQLite operation: {e}; retry only after checking activation receipt"
    ))
}
pub(super) async fn stage_path(pool: &SqlitePool, id: &str) -> Result<PathBuf> {
    let rows = sqlx::query("PRAGMA database_list")
        .fetch_all(pool)
        .await
        .map_err(error)?;
    let file = rows
        .iter()
        .find(|r| r.get::<String, _>("name") == "main")
        .map(|r| r.get::<String, _>("file"))
        .unwrap_or_default();
    let base = if file.is_empty() {
        std::env::temp_dir().join(format!("axon-graph-{}", uuid::Uuid::new_v4()))
    } else {
        PathBuf::from(file)
            .parent()
            .ok_or_else(|| graph_storage_error("live graph database has no parent"))?
            .join("graph-stages")
    };
    if base.exists()
        && fs::symlink_metadata(&base)
            .map_err(|_| graph_storage_error("cannot inspect private graph stage directory"))?
            .file_type()
            .is_symlink()
    {
        return Err(graph_storage_error(
            "private graph stage directory is a symlink; repair directory ownership",
        ));
    }
    let created = !base.exists();
    fs::create_dir_all(&base)
        .map_err(|_| graph_storage_error("cannot create private graph stage directory"))?;
    #[cfg(unix)]
    if created {
        fs::set_permissions(&base, fs::Permissions::from_mode(0o700))
            .map_err(|_| graph_storage_error("cannot restrict graph stage directory"))?;
    }
    #[cfg(unix)]
    if fs::metadata(&base)
        .map_err(|_| graph_storage_error("cannot inspect graph stage permissions"))?
        .permissions()
        .mode()
        & 0o077
        != 0
    {
        return Err(graph_storage_error(
            "graph stage directory permissions must be private; repair mode to 0700",
        ));
    }
    #[cfg(not(unix))]
    let _ = created;
    Ok(base.join(format!("{id}.sqlite")))
}
pub(super) fn create_private_file(path: &std::path::Path) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path).map_err(|_| {
        graph_storage_error(
            "cannot create exclusive private graph stage file; inspect disposal debt",
        )
    })?;
    Ok(())
}
pub(super) fn check_size(path: &std::path::Path) -> Result<()> {
    let mut size = 0_u64;
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let bytes = fs::metadata(format!("{}{suffix}", path.display()))
            .map(|m| m.len())
            .unwrap_or(0);
        size = size.saturating_add(bytes);
    }
    if size > PHYSICAL_STAGE_LIMIT {
        return Err(graph_storage_error(format!(
            "graph stage physical files exceed 1 GiB budget ({size} > {PHYSICAL_STAGE_LIMIT} bytes); reduce generation size and retry",
        ))
        .with_context("observed_bytes", size.to_string())
        .with_context("limit_bytes", PHYSICAL_STAGE_LIMIT.to_string()));
    }
    Ok(())
}
const TABLES: &[(&str, &str)] = &[
    ("graph_nodes", "node_id"),
    ("graph_edges", "edge_id"),
    ("graph_evidence", "edge_id || ':' || evidence_id"),
    ("graph_aliases", "alias_kind || ':' || alias_value"),
    ("graph_conflicts", "conflict_id"),
];
pub(super) async fn seed(
    live: &SqlitePool,
    scratch: &SqlitePool,
    candidates: &[GraphCandidate],
) -> Result<()> {
    let ids: Vec<String> = candidates
        .iter()
        .flat_map(|c| {
            c.nodes
                .iter()
                .map(|n| crate::merge::resolve_node(n).node_id.0)
        })
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let edge_ids = candidates
        .iter()
        .flat_map(|c| {
            let nodes: std::collections::HashMap<_, _> = c
                .nodes
                .iter()
                .map(|n| (n.stable_key.as_str(), crate::merge::resolve_node(n).node_id))
                .collect();
            c.edges
                .iter()
                .filter_map(|e| {
                    Some(
                        crate::merge::edge_id_for(
                            &e.edge_kind,
                            nodes.get(e.from_stable_key.as_str())?,
                            nodes.get(e.to_stable_key.as_str())?,
                        )
                        .0,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let ids_json = serde_json::to_string(&ids)
        .map_err(|_| graph_storage_error("graph seed IDs serialization failed"))?;
    let edges_json = serde_json::to_string(&edge_ids)
        .map_err(|_| graph_storage_error("graph seed edge IDs serialization failed"))?;
    let mut snapshot = live.begin().await.map_err(error)?;
    let mut target = scratch.begin().await.map_err(error)?;
    let mut charged = 0u64;
    for (table, key) in TABLES {
        let cols = sqlx::query(&format!("PRAGMA table_info({table})"))
            .fetch_all(&mut *snapshot)
            .await
            .map_err(error)?
            .iter()
            .map(|r| r.get::<String, _>("name"))
            .collect::<Vec<_>>();
        let objects = cols
            .iter()
            .map(|c| format!("'{c}',{c}"))
            .collect::<Vec<_>>()
            .join(",");
        let filter = match *table {
            "graph_nodes" | "graph_aliases" => {
                "node_id IN (SELECT value FROM json_each(?1)) AND ?2 IS NOT NULL"
            }
            "graph_edges" | "graph_evidence" => "edge_id IN (SELECT value FROM json_each(?2))",
            _ => {
                "target_id IN (SELECT value FROM json_each(?1)) OR target_id IN (SELECT value FROM json_each(?2))"
            }
        };
        let query = format!(
            "SELECT {key} AS row_key,json_object({objects}) AS image FROM {table} WHERE {filter}"
        );
        let q = sqlx::query(&query).bind(&ids_json).bind(&edges_json);
        let mut rows = q.fetch(&mut *snapshot);
        let extracts = cols
            .iter()
            .map(|c| format!("json_extract(value,'$.{c}')"))
            .collect::<Vec<_>>()
            .join(",");
        let mut images = Vec::new();
        while let Some(row) = rows.try_next().await.map_err(error)? {
            let image: String = row.get("image");
            charged += image.len() as u64;
            if charged > LIMIT {
                return Err(graph_storage_error(
                    "graph stage seed exceeds 256 MiB; reduce generation size",
                ));
            }
            images.push(
                serde_json::from_str::<serde_json::Value>(&image)
                    .map_err(|_| graph_storage_error("graph seed row image invalid"))?,
            );
            if images.len() == 128 {
                insert_seed_batch(&mut target, table, &cols, &extracts, &images).await?;
                images.clear();
            }
        }
        if !images.is_empty() {
            insert_seed_batch(&mut target, table, &cols, &extracts, &images).await?;
        }
    }
    target.commit().await.map_err(error)?;
    snapshot.commit().await.map_err(error)?;
    Ok(())
}
pub(super) async fn replay_journal(
    pool: &SqlitePool,
    conn: &mut SqliteConnection,
    source: &SourceId,
) -> Result<GraphWriteResult> {
    let mut rows = sqlx::query_scalar::<_, String>(
        "SELECT candidate_json FROM stage_journal ORDER BY sequence",
    )
    .fetch(pool);
    let mut summary = summary(source, &[]);
    while let Some(json) = rows.try_next().await.map_err(error)? {
        let candidate: GraphCandidate = serde_json::from_str(&json).map_err(|_| {
            graph_storage_error("graph stage journal corrupt; abandon stage and retry generation")
        })?;
        let result = crate::sqlite::upsert::replay_in_tx(conn, &[candidate]).await?;
        summary.candidates_seen += result.candidates_seen;
        summary.nodes_upserted += result.nodes_upserted;
        summary.edges_upserted += result.edges_upserted;
        summary.evidence_records += result.evidence_records;
    }
    Ok(summary)
}
pub(super) fn summary(source: &SourceId, candidates: &[GraphCandidate]) -> GraphWriteResult {
    GraphWriteResult {
        header: crate::sqlite::header::stage_header(),
        source_id: source.clone(),
        candidates_seen: candidates.len() as u64,
        nodes_upserted: candidates.iter().map(|c| c.nodes.len() as u64).sum(),
        edges_upserted: candidates.iter().map(|c| c.edges.len() as u64).sum(),
        evidence_records: candidates
            .iter()
            .map(|c| {
                c.edges
                    .iter()
                    .map(|e| e.evidence_ids.len() as u64)
                    .sum::<u64>()
            })
            .sum(),
        warnings: vec![],
    }
}
pub(super) async fn bulk_apply(conn: &mut SqliteConnection) -> Result<()> {
    for (table, _) in TABLES {
        let cols = sqlx::query(&format!("PRAGMA main.table_info({table})"))
            .fetch_all(&mut *conn)
            .await
            .map_err(error)?
            .iter()
            .map(|r| r.get::<String, _>("name"))
            .collect::<Vec<_>>();
        let keys: &[&str] = match *table {
            "graph_nodes" => &["node_id"],
            "graph_edges" => &["edge_id"],
            "graph_evidence" => &["edge_id", "evidence_id"],
            "graph_aliases" => &["alias_kind", "alias_value"],
            _ => &["conflict_id"],
        };
        let mutable_cols = cols
            .iter()
            .filter(|c| !keys.contains(&c.as_str()) && c.as_str() != "created_at")
            .collect::<Vec<_>>();
        let updates = mutable_cols
            .iter()
            .map(|c| format!("{c}=excluded.{c}"))
            .collect::<Vec<_>>()
            .join(",");
        let changed = mutable_cols
            .iter()
            .map(|c| format!("{table}.{c} IS NOT excluded.{c}"))
            .collect::<Vec<_>>()
            .join(" OR ");
        sqlx::query(&format!("INSERT INTO main.{table} SELECT * FROM axon_stage.{table} WHERE true ON CONFLICT({}) DO UPDATE SET {updates} WHERE {changed}",keys.join(","))).execute(&mut *conn).await.map_err(error)?;
    }
    Ok(())
}
pub(super) async fn reap(pool: &SqlitePool, ids: &[String]) -> Result<usize> {
    let mut count = 0;
    for id in ids {
        if uuid::Uuid::parse_str(id).is_err() {
            return Err(graph_storage_error("invalid opaque graph stage ID"));
        }
        let path: Option<String> = sqlx::query_scalar(
            "SELECT path FROM graph_stages WHERE stage_id=? AND state='disposable'",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(error)?;
        let Some(path) = path else {
            continue;
        };
        let path = PathBuf::from(path);
        validate_disposal_path(pool, &path, id).await?;
        let _owner = match owner_lock(&path) {
            Ok(owner) => owner,
            Err(error)
                if error
                    .details
                    .get("owner_active")
                    .is_some_and(|v| v == "true") =>
            {
                continue;
            }
            Err(error) => return Err(error),
        };
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let p = PathBuf::from(format!("{}{suffix}", path.display()));
            if let Ok(meta) = fs::symlink_metadata(&p) {
                if !meta.is_file() || meta.file_type().is_symlink() {
                    return Err(graph_storage_error(
                        "graph stage disposal refuses nonregular file",
                    ));
                }
                fs::remove_file(p)
                    .map_err(|_| graph_storage_error("graph stage disposal failed; retry debt"))?;
            }
        }
        fs::remove_file(path.with_extension("lock"))
            .map_err(|_| graph_storage_error("graph stage lock disposal failed; retry debt"))?;
        sqlx::query("DELETE FROM graph_stages WHERE stage_id=? AND state='disposable'")
            .bind(id)
            .execute(pool)
            .await
            .map_err(error)?;
        count += 1;
    }
    Ok(count)
}

pub(super) fn owner_lock(path: &std::path::Path) -> Result<fs::File> {
    let lock = path.with_extension("lock");
    if fs::symlink_metadata(&lock).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink()) {
        return Err(graph_storage_error(
            "graph stage lock is not a regular file",
        ));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options
        .open(lock)
        .map_err(|_| graph_storage_error("cannot open graph stage owner lock"))?;
    file.try_lock().map_err(|error| match error {
        fs::TryLockError::WouldBlock => graph_storage_error(
            "graph stage is owned by an active writer; retry disposal after it settles",
        )
        .with_context("owner_active", "true"),
        _ => graph_storage_error(
            "cannot acquire graph stage owner lock; inspect file permissions and retry disposal",
        ),
    })?;
    Ok(file)
}

async fn insert_seed_batch(
    target: &mut SqliteConnection,
    table: &str,
    cols: &[String],
    extracts: &str,
    images: &[serde_json::Value],
) -> Result<()> {
    let json = serde_json::to_string(images)
        .map_err(|_| graph_storage_error("graph seed batch serialization failed"))?;
    sqlx::query(&format!(
        "INSERT OR IGNORE INTO {table}({}) SELECT {extracts} FROM json_each(?)",
        cols.join(",")
    ))
    .bind(json)
    .execute(target)
    .await
    .map_err(error)?;
    Ok(())
}

pub(super) async fn disable_private_revision_triggers(pool: &SqlitePool) -> Result<()> {
    let mut sql = String::new();
    for table in ["nodes", "edges", "evidence", "aliases", "conflicts"] {
        for op in ["insert", "update", "delete"] {
            sql.push_str(&format!(
                "DROP TRIGGER IF EXISTS graph_revision_{table}_{op};"
            ));
        }
    }
    sqlx::raw_sql(&sql).execute(pool).await.map_err(error)?;
    Ok(())
}

pub(super) async fn digests(pool: &SqlitePool) -> Result<(String, String)> {
    use sha2::{Digest, Sha256};
    let mut journal = Sha256::new();
    let mut rows = sqlx::query_scalar::<_, String>(
        "SELECT candidate_json FROM stage_journal ORDER BY sequence",
    )
    .fetch(pool);
    while let Some(json) = rows.try_next().await.map_err(error)? {
        journal.update((json.len() as u64).to_le_bytes());
        journal.update(json.as_bytes());
    }
    drop(rows);
    let mut writes = Sha256::new();
    for (table, key) in TABLES {
        writes.update(table.as_bytes());
        let cols = sqlx::query(&format!("PRAGMA table_info({table})"))
            .fetch_all(pool)
            .await
            .map_err(error)?
            .iter()
            .map(|r| r.get::<String, _>("name"))
            .collect::<Vec<_>>();
        let objects = cols
            .iter()
            .map(|c| format!("'{c}',{c}"))
            .collect::<Vec<_>>()
            .join(",");
        let query = format!("SELECT json_object({objects}) FROM {table} ORDER BY {key}");
        let mut rows = sqlx::query_scalar::<_, String>(&query).fetch(pool);
        while let Some(json) = rows.try_next().await.map_err(error)? {
            writes.update((json.len() as u64).to_le_bytes());
            writes.update(json.as_bytes());
        }
    }
    Ok((
        format!("{:x}", journal.finalize()),
        format!("{:x}", writes.finalize()),
    ))
}

pub(super) async fn validate_disposal_path(
    pool: &SqlitePool,
    path: &std::path::Path,
    id: &str,
) -> Result<()> {
    if path.file_name().and_then(|s| s.to_str()) != Some(format!("{id}.sqlite").as_str())
        || path.parent().is_none()
    {
        return Err(graph_storage_error(
            "graph stage disposal path does not match ownership",
        ));
    }
    let directory = path.parent().unwrap();
    let rows = sqlx::query("PRAGMA database_list")
        .fetch_all(pool)
        .await
        .map_err(error)?;
    let main = rows
        .iter()
        .find(|r| r.get::<String, _>("name") == "main")
        .map(|r| r.get::<String, _>("file"))
        .unwrap_or_default();
    let expected = PathBuf::from(main)
        .parent()
        .map(|p| p.join("graph-stages").join(format!("{id}.sqlite")));
    let memory = directory
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.starts_with("axon-graph-"))
        && directory.parent() == Some(std::env::temp_dir().as_path());
    if Some(path) != expected.as_deref() && !memory {
        return Err(graph_storage_error(
            "graph stage disposal path outside private database directory",
        ));
    }
    if fs::symlink_metadata(directory)
        .map_err(|_| graph_storage_error("cannot inspect stage directory"))?
        .file_type()
        .is_symlink()
    {
        return Err(graph_storage_error(
            "graph stage disposal directory is a symlink",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
