//! Cutover store-inventory detection for `axon doctor`.
//!
//! The clean-slate cutover contract requires doctor to detect an incompatible
//! or non-empty store and recommend `axon reset` before unified workers start
//! (`docs/pipeline-unification/delivery/cutover-contract.md` — "axon doctor must
//! detect incompatible non-empty stores and recommend wiping/reinitializing").
//!
//! This runs core-only probes (read-only SQLite queries + Qdrant HTTP) so it
//! stays in `axon-core` below the services layer. It never mutates.

use crate::config::Config;
use axon_api::reset::TARGET_PAYLOAD_CONTRACT_VERSION;
use serde_json::Value;
use std::time::Duration;

const CUTOVER_QDRANT_TIMEOUT: Duration = Duration::from_secs(2);

pub async fn assert_workers_allowed_by_cutover(cfg: &Config) -> Result<(), String> {
    if std::env::var("AXON_ALLOW_INCOMPATIBLE_STORE_STARTUP")
        .ok()
        .is_some_and(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
    {
        return Ok(());
    }

    let sqlite_ok = true;
    let qdrant_reachable = !cfg.qdrant_url.trim().is_empty();
    let report = build_cutover_block(cfg, qdrant_reachable, sqlite_ok).await;
    let blocked = report
        .get("startup_blocked")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !blocked {
        return Ok(());
    }
    let guidance = report
        .get("guidance")
        .and_then(Value::as_str)
        .unwrap_or("run `axon reset --dry-run`, review the plan, then `axon reset --yes`");
    Err(format!("startup.incompatible_store: {guidance}"))
}

/// Build the `cutover_stores` doctor block: per-store non-empty / incompatible
/// flags plus a top-level `reset_recommended` signal and guidance.
///
/// `qdrant_reachable` / `sqlite_ok` are the results the caller already probed so
/// this does not re-run those; `collection_url` is the raw Qdrant base.
pub async fn build_cutover_block(cfg: &Config, qdrant_reachable: bool, sqlite_ok: bool) -> Value {
    let sqlite = sqlite_store_status(cfg, sqlite_ok).await;
    let vectors = qdrant_store_status(cfg, qdrant_reachable).await;

    let sqlite_non_empty = sqlite
        .get("non_empty")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let sqlite_incompatible = sqlite
        .get("schema_incompatible")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let vectors_non_empty = vectors
        .get("non_empty")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let vectors_incompatible = vectors
        .get("schema_incompatible")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let reset_recommended = sqlite_non_empty || sqlite_incompatible || vectors_incompatible;
    let guidance = if reset_recommended {
        Some(reset_guidance(
            sqlite_non_empty || sqlite_incompatible,
            vectors_non_empty,
            vectors_incompatible,
        ))
    } else {
        None
    };

    serde_json::json!({
        "reset_recommended": reset_recommended,
        "startup_blocked": reset_recommended,
        "empty_and_fresh": !reset_recommended,
        "guidance": guidance,
        "stores": {
            "sqlite": sqlite,
            "vectors": vectors,
        },
    })
}

fn reset_guidance(sqlite: bool, vectors_non_empty: bool, vectors_incompatible: bool) -> String {
    let mut reasons = Vec::new();
    if sqlite {
        reasons.push("SQLite store has a pre-cutover schema or retired content rows");
    }
    if vectors_incompatible {
        reasons.push("Qdrant collection carries an old payload schema");
    } else if vectors_non_empty {
        reasons.push("Qdrant collection is non-empty");
    }
    format!(
        "{}. Run `axon reset` (dry-run first, then `axon reset --yes`) to wipe and reinitialize \
         local stores at the fresh schema before starting unified workers.",
        reasons.join("; ")
    )
}

/// Read-only SQLite inventory: does the DB hold retired pre-cutover rows.
async fn sqlite_store_status(cfg: &Config, sqlite_ok: bool) -> Value {
    let path = &cfg.sqlite_path;
    if !path.exists() {
        return serde_json::json!({
            "exists": false,
            "non_empty": false,
            "content_rows": 0,
            "note": "fresh — SQLite DB not yet created",
        });
    }
    let (legacy_rows, error) = match count_sqlite_legacy_rows(path).await {
        Ok(rows) => (rows, None),
        Err(e) => (0, Some(e)),
    };
    let (schema_incompatible, schema_error) =
        match sqlite_schema_identity_is_incompatible(path).await {
            Ok(incompatible) => (incompatible, None),
            Err(error) => (true, Some(error)),
        };
    serde_json::json!({
        "exists": true,
        "ok": sqlite_ok,
        "non_empty": legacy_rows > 0,
        "schema_incompatible": schema_incompatible,
        "legacy_rows": legacy_rows,
        "content_rows": legacy_rows,
        "probe_error": error,
        "schema_probe_error": schema_error,
    })
}

/// Reject any populated SQLite file that does not carry the clean-break epoch
/// and receipt-ledger shape. The jobs runner performs exact receipt checksum,
/// table, and FK validation; doctor keeps this lower-layer probe read-only.
async fn sqlite_schema_identity_is_incompatible(path: &std::path::Path) -> Result<bool, String> {
    use sqlx::{Row, SqlitePool};
    let connect = format!("sqlite://{}?mode=ro", path.display());
    let pool = SqlitePool::connect(&connect)
        .await
        .map_err(|error| error.to_string())?;
    let user_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(&pool)
    .await
    .map_err(|error| error.to_string())?;
    if user_tables == 0 {
        pool.close().await;
        return Ok(false);
    }

    let epoch: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .map_err(|error| error.to_string())?;
    let columns: std::collections::BTreeSet<String> =
        sqlx::query("PRAGMA table_info('axon_applied_migrations')")
            .fetch_all(&pool)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|row| row.get::<String, _>("name"))
            .collect();
    let expected = std::collections::BTreeSet::from([
        "namespace".to_string(),
        "version".to_string(),
        "name".to_string(),
        "checksum".to_string(),
        "schema_epoch".to_string(),
        "applied_at".to_string(),
    ]);
    pool.close().await;
    Ok(epoch != 1 || columns != expected)
}

/// Sum rows across retired pre-cutover tables that exist. Read-only
/// (`mode=ro`), best-effort — a missing table contributes zero.
///
/// Current unified tables (`sources`, `memory_records`, `graph_nodes`, `jobs`,
/// etc.) are intentionally EXCLUDED. They are valid post-cutover runtime state;
/// counting them made the second CLI command after a fresh successful write trip
/// `startup.incompatible_store`.
async fn count_sqlite_legacy_rows(path: &std::path::Path) -> Result<u64, String> {
    use sqlx::SqlitePool;
    let connect = format!("sqlite://{}?mode=ro", path.display());
    let pool = SqlitePool::connect(&connect)
        .await
        .map_err(|e| e.to_string())?;
    let mut total: u64 = 0;
    for table in [
        "axon_source_sources",
        "axon_source_manifest_items",
        "axon_source_cleanup_debt",
        "axon_crawl_jobs",
        "axon_embed_jobs",
        "axon_extract_jobs",
        "axon_ingest_jobs",
        "axon_ingest_payloads",
        "axon_code_index_generations",
        "axon_code_index_files",
        "axon_watch_defs",
        "axon_watch_runs",
        "axon_session_watch_defs",
        "axon_session_watch_runs",
    ] {
        let present: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
        if present == 0 {
            continue;
        }
        // Fixed allowlist table names — not user input.
        let sql = format!("SELECT COUNT(*) FROM \"{table}\"");
        let count: i64 = sqlx::query_scalar(&sql).fetch_one(&pool).await.unwrap_or(0);
        total = total.saturating_add(count.max(0) as u64);
    }
    pool.close().await;
    Ok(total)
}

/// Qdrant point-count + payload-schema-compatibility inventory for the
/// configured collection.
async fn qdrant_store_status(cfg: &Config, reachable: bool) -> Value {
    if !reachable {
        return serde_json::json!({
            "reachable": false,
            "non_empty": false,
            "schema_incompatible": false,
            "note": "Qdrant unreachable — store not inventoried",
        });
    }
    let client = match crate::http::internal_service_http_client() {
        Ok(c) => c,
        Err(e) => {
            return serde_json::json!({
                "reachable": false,
                "non_empty": false,
                "schema_incompatible": false,
                "probe_error": e.to_string(),
            });
        }
    };
    let base = cfg.qdrant_url.trim_end_matches('/');
    let collection = &cfg.collection;

    // Existence: collection GET 404 ⇒ fresh/empty.
    let info_url = format!("{base}/collections/{collection}");
    match tokio::time::timeout(CUTOVER_QDRANT_TIMEOUT, client.get(&info_url).send()).await {
        Err(_) => {
            return serde_json::json!({
                "reachable": false,
                "non_empty": false,
                "schema_incompatible": false,
                "probe_error": "collection GET timed out",
            });
        }
        Ok(Ok(r)) if r.status() == reqwest::StatusCode::NOT_FOUND => {
            return serde_json::json!({
                "reachable": true,
                "exists": false,
                "non_empty": false,
                "schema_incompatible": false,
                "note": "fresh — collection does not exist",
            });
        }
        Ok(Ok(r)) if r.status().is_success() => {}
        Ok(Ok(r)) => {
            return serde_json::json!({
                "reachable": true,
                "non_empty": false,
                "schema_incompatible": false,
                "probe_error": format!("collection GET {}", r.status()),
            });
        }
        Ok(Err(e)) => {
            return serde_json::json!({
                "reachable": false,
                "non_empty": false,
                "schema_incompatible": false,
                "probe_error": e.to_string(),
            });
        }
    }
    let points = qdrant_point_count(client, base, collection).await;
    let (sampled_contract_versions, incompatible) = if points > 0 {
        qdrant_contract_versions(client, base, collection).await
    } else {
        (Vec::new(), false)
    };

    serde_json::json!({
        "reachable": true,
        "exists": true,
        "collection": collection,
        "points": points,
        "non_empty": points > 0,
        "payload_contract_versions": sampled_contract_versions,
        "target_payload_contract_version": TARGET_PAYLOAD_CONTRACT_VERSION,
        "schema_incompatible": incompatible,
    })
}

async fn qdrant_point_count(client: &reqwest::Client, base: &str, collection: &str) -> u64 {
    let url = format!("{base}/collections/{collection}/points/count");
    match tokio::time::timeout(
        CUTOVER_QDRANT_TIMEOUT,
        client
            .post(&url)
            .json(&serde_json::json!({ "exact": true }))
            .send(),
    )
    .await
    {
        Ok(Ok(resp)) if resp.status().is_success() => resp
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| v.pointer("/result/count").and_then(Value::as_u64))
            .unwrap_or(0),
        _ => 0,
    }
}

/// Scroll a bounded sample and return observed `payload_contract_version`
/// values plus whether any sampled point is not on the current contract.
///
/// Missing `payload_contract_version` is incompatible: those are
/// pre-unification points and must not be reused for current-contract writes.
async fn qdrant_contract_versions(
    client: &reqwest::Client,
    base: &str,
    collection: &str,
) -> (Vec<String>, bool) {
    let url = format!("{base}/collections/{collection}/points/scroll");
    let body = serde_json::json!({
        "limit": 256,
        "with_payload": ["payload_contract_version"],
        "with_vector": false,
    });
    let page: Value =
        match tokio::time::timeout(CUTOVER_QDRANT_TIMEOUT, client.post(&url).json(&body).send())
            .await
        {
            Ok(Ok(resp)) if resp.status().is_success() => match resp.json().await {
                Ok(v) => v,
                Err(_) => return (Vec::new(), false),
            },
            _ => return (Vec::new(), false),
        };
    let points = match page.pointer("/result/points").and_then(Value::as_array) {
        Some(p) if !p.is_empty() => p,
        _ => return (Vec::new(), false),
    };
    let mut versions = std::collections::BTreeSet::new();
    let mut incompatible = false;
    for point in points {
        match point
            .pointer("/payload/payload_contract_version")
            .and_then(Value::as_str)
        {
            Some(version) => {
                versions.insert(version.to_string());
                if version != TARGET_PAYLOAD_CONTRACT_VERSION {
                    incompatible = true;
                }
            }
            None => {
                versions.insert("<missing>".to_string());
                incompatible = true;
            }
        }
    }
    (versions.into_iter().collect(), incompatible)
}

#[cfg(test)]
#[path = "cutover_tests.rs"]
mod tests;
