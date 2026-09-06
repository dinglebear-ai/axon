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
//! `config_snapshot_id` is a deterministic hash of `config_json`. This module
//! recomputes that identity and rejects forged/conflicting pairs. Canonical
//! job creation passes material through `JobStore::create_with_config_snapshot`,
//! which persists the snapshot and referencing job in one transaction without
//! exposing snapshot material on the transport DTO.

use axon_api::source::*;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::boundary::Result;
use crate::unified_codec::{now_timestamp, sql_error};

/// Idempotently store a config snapshot's serialized content by id.
///
/// The id is recomputed from `config_json`; forged ids and existing rows bound
/// to different bytes are rejected.
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
    let expected_id = config_snapshot_id_from_json(config_json);
    if config_snapshot_id != expected_id {
        return Err(ApiError::new(
            "config_snapshot.digest_mismatch",
            ErrorStage::Publishing,
            format!(
                "config snapshot id {config_snapshot_id} does not match its content digest {expected_id}"
            ),
        ));
    }
    let now = now_timestamp();
    let result = sqlx::query(
        "INSERT INTO config_snapshots (config_snapshot_id, config_json, created_at) VALUES (?, ?, ?) \
         ON CONFLICT(config_snapshot_id) DO NOTHING",
    )
    .bind(config_snapshot_id)
    .bind(config_json)
    .bind(now.0.as_str())
    .execute(pool)
    .await
    .map_err(sql_error)?;
    if result.rows_affected() == 0 {
        let stored: String = sqlx::query_scalar(
            "SELECT config_json FROM config_snapshots WHERE config_snapshot_id = ?",
        )
        .bind(config_snapshot_id)
        .fetch_one(pool)
        .await
        .map_err(sql_error)?;
        if stored != config_json {
            return Err(ApiError::new(
                "config_snapshot.digest_mismatch",
                ErrorStage::Publishing,
                format!(
                    "config snapshot id {config_snapshot_id} is already bound to different content"
                ),
            ));
        }
    }
    Ok(())
}

/// Derive the canonical content-addressed identifier for serialized snapshot material.
pub fn config_snapshot_id_from_json(config_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(config_json.as_bytes());
    let digest = hasher.finalize();
    let short = digest[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("cfg_{short}")
}

/// Fetch a previously stored config snapshot's raw JSON by id.
///
/// Returns `Ok(None)` for an id that was never stored.
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

/// Remove credential-bearing custom headers written by versions predating the
/// durable snapshot boundary. Legacy identifiers remain stable so referencing
/// jobs stay valid; all newly-created snapshots remain content-addressed.
pub(crate) async fn scrub_legacy_snapshot_credentials(pool: &SqlitePool) -> Result<()> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT config_snapshot_id, config_json FROM config_snapshots",
    )
    .fetch_all(pool)
    .await
    .map_err(sql_error)?;
    for (id, encoded) in rows {
        let mut value = serde_json::from_str::<serde_json::Value>(&encoded).map_err(|_| {
            ApiError::new(
                "config_snapshot.redaction_failed",
                ErrorStage::Authorizing,
                "legacy config snapshot could not be safely inspected",
            )
        })?;
        let Some(headers) = value
            .get_mut("custom_headers")
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };
        let before = headers.len();
        headers.retain(|header| {
            header
                .as_str()
                .is_some_and(|header| !header_contains_credential(header))
        });
        if headers.len() == before {
            continue;
        }
        let scrubbed = serde_json::to_string(&value).map_err(|error| {
            ApiError::new(
                "config_snapshot.redaction_failed",
                ErrorStage::Publishing,
                error.to_string(),
            )
        })?;
        sqlx::query("UPDATE config_snapshots SET config_json = ? WHERE config_snapshot_id = ?")
            .bind(scrubbed)
            .bind(id)
            .execute(pool)
            .await
            .map_err(sql_error)?;
    }
    Ok(())
}

fn header_contains_credential(header: &str) -> bool {
    let name = header.split_once(':').map_or(header, |(name, _)| name);
    let normalized = name.trim().to_ascii_lowercase().replace('_', "-");
    normalized == "authorization"
        || normalized == "proxy-authorization"
        || normalized == "cookie"
        || normalized == "set-cookie"
        || normalized.contains("api-key")
        || normalized.contains("apikey")
        || normalized.contains("token")
        || normalized.contains("secret")
}

#[cfg(test)]
#[path = "config_snapshot_store_tests.rs"]
mod tests;
