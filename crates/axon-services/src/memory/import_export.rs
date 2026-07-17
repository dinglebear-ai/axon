//! `memory import`/`memory export` — bulk record transfer.
//!
//! Bypasses the flat [`MemoryRequest`]/[`dispatch`] shape (unlike every other
//! subaction in `super`): import carries a `records` bundle and export returns
//! one, neither of which fit the single-memory request/response shape the rest
//! of the `memory` surface uses. Transport-neutral entry points consumed
//! directly by REST `POST /v1/memories/import` / `POST /v1/memories/export`
//! and the MCP `memory` `import`/`export` subactions.

use super::*;
use anyhow::bail;
use axon_api::source::{
    ArtifactKind, ArtifactRef, MemoryExportRequest, MemoryExportResult, MemoryImportMode,
    MemoryImportRequest, MemoryImportResult, MemoryScope, MemoryStatus, MetadataMap, Timestamp,
    Visibility,
};
use axon_core::boundary::{ArtifactBytesWriteRequest, ArtifactStore, FileArtifactStore};
use sha2::{Digest, Sha256};

/// Hard cap on the number of records accepted by a single import request.
///
/// The 10 MiB `MEMORY_IMPORT_EXPORT_BODY_LIMIT` (see `axon-web`) bounds wire
/// size but not record count — a 10 MiB body of minimal `MemoryRecord`
/// objects can still contain tens of thousands of records, each triggering
/// embedding + upsert work synchronously in one request. This cap bounds that
/// work independent of body size.
pub const MAX_MEMORY_IMPORT_RECORDS: usize = 5_000;

/// Caller authorization for memory-import modes that mutate beyond the
/// records supplied in the request.
///
/// Mirrors [`axon_prune::PruneAuthz`]'s shape and rationale: this is a small,
/// explicit authz value threaded in by the caller (CLI/MCP/REST) from the
/// caller's real auth context — never hardcoded/assumed inside this module.
/// `MemoryImportMode::ReplaceScope` archives every existing memory in the
/// target scope before importing and is documented as requiring
/// `axon:admin` at the transport boundary
/// ([`axon_api::source::MemoryImportMode::ReplaceScope`]); this struct is how
/// that requirement is actually enforced.
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryAuthz {
    pub is_admin: bool,
}

impl MemoryAuthz {
    pub fn admin() -> Self {
        Self { is_admin: true }
    }

    pub fn anonymous() -> Self {
        Self { is_admin: false }
    }
}

/// Bulk-import memory records (or preview a dry-run plan).
///
/// `authz` gates [`MemoryImportMode::ReplaceScope`]: only a caller resolved
/// as admin (`axon:admin`) may mass-archive an existing scope. `Merge` mode
/// has no elevated requirement.
pub async fn import(
    ctx: &ServiceContext,
    req: MemoryImportRequest,
    authz: &MemoryAuthz,
) -> Result<MemoryImportResult> {
    if req.mode == MemoryImportMode::ReplaceScope && !authz.is_admin {
        bail!(
            "memory import mode 'replace_scope' requires axon:admin \
             (archives every existing memory in the target scope before importing)"
        );
    }
    if req.records.len() > MAX_MEMORY_IMPORT_RECORDS {
        bail!(
            "memory import exceeds the maximum of {MAX_MEMORY_IMPORT_RECORDS} records \
             ({} supplied); split the import into smaller batches",
            req.records.len()
        );
    }
    let store = memory_store(ctx).await?;
    // Job-tracked like `compact` (contract R3-16: memory jobs pollable via
    // `job_id`) — `request_json`'s `payload` is the full typed
    // `MemoryImportRequest` so a detached unified-worker claim of this same
    // `memory_import` job (`crate::runtime::job_runners::
    // MemoryCompactionRunner`) can reconstruct and execute it independently
    // of this foreground call.
    let request_json = json!({
        "operation": "memory_import",
        "payload": serde_json::to_value(&req).context("serialize import request")?,
    });
    job_tracking::track_operation_job(
        ctx,
        axon_api::source::OperationKind::MemoryImport,
        request_json,
        || async move {
            let mut sync_ids = replaced_scope_memory_ids(store.as_ref(), &req).await?;
            let result = store.import(req).await.map_err(store_err)?;
            sync_ids.extend(result.created_ids.iter().cloned());
            if !result.dry_run && !sync_ids.is_empty() {
                super::sync::sync_memory_records(ctx, store.as_ref(), sync_ids, "import").await?;
            }
            Ok(result)
        },
    )
    .await
}

pub(crate) async fn replaced_scope_memory_ids(
    store: &dyn MemoryStore,
    request: &MemoryImportRequest,
) -> Result<Vec<axon_api::source::MemoryId>> {
    if request.mode != MemoryImportMode::ReplaceScope || request.dry_run {
        return Ok(Vec::new());
    }
    let scopes = request
        .records
        .iter()
        .map(|record| (record.scope.kind.clone(), record.scope.value.clone()))
        .collect::<std::collections::BTreeSet<_>>();
    let mut ids = Vec::new();
    for (kind, value) in scopes {
        let existing = store
            .export(MemoryExportRequest {
                scope: Some(MemoryScope { kind, value }),
                include_archived: true,
                include_working: true,
            })
            .await
            .map_err(store_err)?;
        ids.extend(
            existing
                .records
                .into_iter()
                .filter(|record| record.status != MemoryStatus::Archived)
                .map(|record| record.memory_id),
        );
    }
    Ok(ids)
}

/// Export memory records matching a scope.
///
/// Contract "Import and Export": "export writes an artifact or stream with
/// redacted content according to caller scope." Two things happen beyond the
/// raw store call:
/// - caller-scope filtering: `sensitive`-visibility records are dropped
///   unless `authz.is_admin` (bodies are already secret-redacted at write
///   time — see `MemoryRecord::visibility` and `RedactionContext::
///   memory_record()` — so this is a classification-level access gate, not a
///   second content scrub).
/// - artifact backing: the filtered record set is written through the
///   artifact boundary under `cfg.output_dir/artifacts`, and the result carries the resulting
///   `ArtifactRef` so REST/CLI/MCP callers get a durable, hashable export
///   even when the response body itself is also returned inline.
pub async fn export(
    ctx: &ServiceContext,
    req: MemoryExportRequest,
    authz: &MemoryAuthz,
) -> Result<MemoryExportResult> {
    let store = memory_store(ctx).await?;
    let mut result = store.export(req).await.map_err(store_err)?;

    if !authz.is_admin {
        result
            .records
            .retain(|record| record.visibility != Visibility::Sensitive);
    }
    result.count = result.records.len() as u32;

    let payload = serde_json::to_vec_pretty(&result.records)
        .context("serialize exported memory records for artifact write")?;
    let mut hasher = Sha256::new();
    hasher.update(&payload);
    let content_hash = format!("{:x}", hasher.finalize());
    let size_bytes = payload.len() as u64;
    let handle = FileArtifactStore::new(ctx.cfg().output_dir.join("artifacts"))
        .put_bytes(ArtifactBytesWriteRequest {
            kind: ArtifactKind::Report,
            content_type: "application/json".to_string(),
            bytes: payload,
            source_id: None,
            job_id: None,
            metadata: MetadataMap::new(),
        })
        .await
        .map_err(|err| anyhow::anyhow!("write memory export artifact: {err}"))?;

    result.artifact = Some(ArtifactRef {
        artifact_id: handle.artifact_id,
        artifact_kind: ArtifactKind::Report,
        uri: handle.uri.unwrap_or_default(),
        size_bytes: Some(size_bytes),
        content_hash: Some(content_hash),
        created_at: Timestamp::from(chrono::Utc::now()),
    });

    Ok(result)
}

/// Build a [`MemoryImportRequest`] from the flat CLI/MCP [`MemoryRequest`]
/// shape (`dispatch`'s `MemorySubaction::Import` arm).
pub(crate) fn import_request_from_flat(req: MemoryRequest) -> Result<MemoryImportRequest> {
    let records = req
        .records
        .filter(|records| !records.is_empty())
        .context("import requires records (at least 1)")?;
    Ok(MemoryImportRequest {
        records,
        mode: req.import_mode.unwrap_or(MemoryImportMode::Merge),
        dry_run: req.dry_run.unwrap_or(false),
    })
}

/// Build a [`MemoryExportRequest`] from the flat CLI/MCP [`MemoryRequest`]
/// shape (`dispatch`'s `MemorySubaction::Export` arm).
pub(crate) fn export_request_from_flat(req: MemoryRequest) -> MemoryExportRequest {
    MemoryExportRequest {
        scope: req.export_scope,
        include_archived: req.include_archived.unwrap_or(false),
        include_working: req.include_working.unwrap_or(false),
    }
}
