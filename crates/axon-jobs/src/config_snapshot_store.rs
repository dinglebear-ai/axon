//! Real store/read backing for the `config_snapshots` table (migration
//! `0025_config_snapshots.sql`).
//!
//! `docs/pipeline-unification/schemas/database-schema.md`'s "Required
//! Tables" registry and `docs/pipeline-unification/runtime/schema-contract.md`
//! both list `config_snapshots` (owned by `axon-jobs`, PK
//! `config_snapshot_id`) as a canonical target table — see the migration's
//! own doc comment for the full contract citation. This module is the
//! store/read API the audit finding asked for.
//!
//! `config_snapshot_id` is a deterministic hash of `config_json` (see
//! `axon-services::config_snapshot_hash`), so [`upsert_config_snapshot`] is a
//! plain `INSERT OR IGNORE`: the same id can only ever pair with the same
//! content, so a duplicate write from a second job is a no-op, not an error.
//!
//! **Not yet called from the unified job-creation path**
//! (`unified::ops::create_job`): `JobCreateRequest` (the shared CLI/MCP/REST
//! wire DTO) carries only `config_snapshot_id`, not the JSON body that
//! produced it, and most of the ~15 job-kind builders that stamp a
//! `config_snapshot_id` onto a `JobCreateRequest` derive it from small
//! per-kind "material" structs (`axon-services::config_snapshot_hash::JobConfigSnapshot`),
//! not the full `Config` — so there isn't always a JSON body available to
//! store at that call site today. Wiring every builder to also persist its
//! material is a deliberate follow-up needing either a `JobCreateRequest` DTO
//! field (touches the CLI/MCP/REST wire contract and OpenAPI) or a new
//! `ServiceJobRuntime` method threaded through each builder — a separate,
//! properly-scoped change, not a same-pass rewrite of the wire contract.

use axon_api::source::*;
use sqlx::SqlitePool;

use crate::boundary::Result;
use crate::unified_codec::{now_timestamp, sql_error};

/// Idempotently store a config snapshot's serialized content by id.
///
/// `config_snapshot_id` is expected to be the deterministic hash of
/// `config_json` (see `axon-services::config_snapshot_hash`); this function
/// does not itself verify that pairing — a caller that mismatches id and
/// content will silently keep whichever content was written first for that
/// id (matching the "content-addressed" contract documented on the
/// migration).
pub async fn upsert_config_snapshot(
    pool: &SqlitePool,
    config_snapshot_id: &str,
    config_json: &str,
) -> Result<()> {
    if config_snapshot_id.trim().is_empty() {
        return Err(ApiError::new(
            "config_snapshot.invalid_id",
            ErrorStage::Publishing,
            "config_snapshot_id must not be empty",
        ));
    }
    let now = now_timestamp();
    sqlx::query(
        "INSERT OR IGNORE INTO config_snapshots (config_snapshot_id, config_json, created_at) \
         VALUES (?, ?, ?)",
    )
    .bind(config_snapshot_id)
    .bind(config_json)
    .bind(now.0.as_str())
    .execute(pool)
    .await
    .map_err(sql_error)?;
    Ok(())
}

/// Fetch a previously stored config snapshot's raw JSON by id.
///
/// Returns `Ok(None)` for an id that was never stored (e.g. any job kind
/// whose builder has not yet been wired to call [`upsert_config_snapshot`] —
/// see the module doc comment) rather than treating that as an error: an
/// unresolved `config_snapshot_id` is a known, current gap, not corruption.
pub async fn get_config_snapshot(
    pool: &SqlitePool,
    config_snapshot_id: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar("SELECT config_json FROM config_snapshots WHERE config_snapshot_id = ?")
        .bind(config_snapshot_id)
        .fetch_optional(pool)
        .await
        .map_err(sql_error)
}

#[cfg(test)]
#[path = "config_snapshot_store_tests.rs"]
mod tests;
