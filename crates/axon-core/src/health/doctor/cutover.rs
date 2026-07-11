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
use axon_api::reset::TARGET_PAYLOAD_SCHEMA_VERSION;
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
    let vectors_non_empty = vectors
        .get("non_empty")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let vectors_incompatible = vectors
        .get("schema_incompatible")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let reset_recommended = sqlite_non_empty || vectors_non_empty || vectors_incompatible;
    let guidance = if reset_recommended {
        Some(reset_guidance(
            sqlite_non_empty,
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
        reasons.push("SQLite store holds pre-cutover ledger/memory/content rows");
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

/// Read-only SQLite content-row inventory: does the DB hold pre-cutover rows.
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
    let (content_rows, error) = match count_sqlite_content_rows(path).await {
        Ok(rows) => (rows, None),
        Err(e) => (0, Some(e)),
    };
    serde_json::json!({
        "exists": true,
        "ok": sqlite_ok,
        "non_empty": content_rows > 0,
        "content_rows": content_rows,
        "probe_error": error,
    })
}

/// Sum rows across the primary content-bearing tables that exist. Read-only
/// (`mode=ro`), best-effort — a missing table contributes zero.
///
/// Job-queue tables are intentionally EXCLUDED. Job rows — whether in the
/// unified `jobs`/`job_events`/`job_artifacts` tables or the legacy
/// `axon_*_jobs` tables — are transient runtime state, not the indexed content
/// a cutover wipe is about. Counting them made a single post-cutover `extract`
/// (or any other) job write trip the `startup.incompatible_store` guard on the
/// very next invocation, forcing an `axon reset` before every run. The genuine
/// pre-cutover signals (non-empty `sources`/`source_documents`/…, or an
/// old Qdrant payload schema) are still detected below and by the vector probe.
async fn count_sqlite_content_rows(path: &std::path::Path) -> Result<u64, String> {
    use sqlx::SqlitePool;
    let connect = format!("sqlite://{}?mode=ro", path.display());
    let pool = SqlitePool::connect(&connect)
        .await
        .map_err(|e| e.to_string())?;
    let mut total: u64 = 0;
    for table in [
        "sources",
        "source_generations",
        "source_documents",
        "source_cleanup_debt",
        "code_index_generations",
        "code_index_files",
        "watches",
        "watch_runs",
        "memory_records",
        "memory_edges",
        "graph_nodes",
        "graph_edges",
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
    let (min_schema_version, incompatible) = if points > 0 {
        qdrant_min_schema(client, base, collection).await
    } else {
        (None, false)
    };

    serde_json::json!({
        "reachable": true,
        "exists": true,
        "collection": collection,
        "points": points,
        "non_empty": points > 0,
        "min_payload_schema_version": min_schema_version,
        "target_payload_schema_version": TARGET_PAYLOAD_SCHEMA_VERSION,
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

/// Scroll a bounded sample and return the minimum `payload_schema_version` plus
/// whether any is older than the target. A missing field is implicit v1.
async fn qdrant_min_schema(
    client: &reqwest::Client,
    base: &str,
    collection: &str,
) -> (Option<u32>, bool) {
    let url = format!("{base}/collections/{collection}/points/scroll");
    let body = serde_json::json!({
        "limit": 256,
        "with_payload": ["payload_schema_version"],
        "with_vector": false,
    });
    let page: Value =
        match tokio::time::timeout(CUTOVER_QDRANT_TIMEOUT, client.post(&url).json(&body).send())
            .await
        {
            Ok(Ok(resp)) if resp.status().is_success() => match resp.json().await {
                Ok(v) => v,
                Err(_) => return (None, false),
            },
            _ => return (None, false),
        };
    let points = match page.pointer("/result/points").and_then(Value::as_array) {
        Some(p) if !p.is_empty() => p,
        _ => return (None, false),
    };
    let mut min_version: Option<u32> = None;
    for point in points {
        let version = point
            .pointer("/payload/payload_schema_version")
            .and_then(Value::as_u64)
            .map(|v| v as u32)
            .unwrap_or(1);
        min_version = Some(min_version.map_or(version, |m| m.min(version)));
    }
    let incompatible = min_version.is_some_and(|v| v < TARGET_PAYLOAD_SCHEMA_VERSION);
    (min_version, incompatible)
}
