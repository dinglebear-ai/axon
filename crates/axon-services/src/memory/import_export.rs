//! `memory import`/`memory export` — bulk record transfer.
//!
//! Bypasses the flat [`MemoryRequest`]/[`dispatch`] shape (unlike every other
//! subaction in `super`): import carries a `records` bundle and export returns
//! one, neither of which fit the single-memory request/response shape the rest
//! of the `memory` surface uses. Transport-neutral entry points consumed
//! directly by REST `POST /v1/memories/import` / `POST /v1/memories/export`
//! and the MCP `memory` `import`/`export` subactions.

use super::*;
use axon_api::source::{
    MemoryExportRequest, MemoryExportResult, MemoryImportMode, MemoryImportRequest,
    MemoryImportResult,
};

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
    store.import(req).await.map_err(store_err)
}

/// Export memory records matching a scope.
pub async fn export(ctx: &ServiceContext, req: MemoryExportRequest) -> Result<MemoryExportResult> {
    let store = memory_store(ctx).await?;
    store.export(req).await.map_err(store_err)
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
    }
}
