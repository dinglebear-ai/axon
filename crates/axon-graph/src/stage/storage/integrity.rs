//! Stable stage journal and canonical write digests.

use super::*;

pub(in crate::stage) async fn digests(pool: &SqlitePool) -> Result<(String, String)> {
    use sha2::{Digest, Sha256};
    let mut journal = Sha256::new();
    let mut rows = sqlx::query_scalar::<_, Option<String>>(
        "SELECT CASE WHEN length(CAST(candidate_json AS BLOB))<=12582912 THEN candidate_json END FROM stage_journal ORDER BY sequence",
    )
    .fetch(pool);
    while let Some(json) = rows.try_next().await.map_err(error)? {
        let json = json.ok_or_else(|| {
            graph_storage_error("graph journal encoded record exceeds admitted window")
        })?;
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
