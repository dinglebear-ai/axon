//! `purge` service facade — delete indexed points by URL (or seed-URL prefix).
//!
//! The delete LOGIC lives in `axon-vector` (it owns the Qdrant data) and the
//! result DTO lives in `axon-api`. This is a **thin facade**, not a
//! reimplementation: it exists so every transport keeps one import surface
//! (`services::system::purge`) and gets the `Box<dyn Error>` error contract.
//! `dry_run` returns a preview (nothing deleted); destructive confirmation is
//! each transport's own concern.
//!
//! Per the pruning contract (`docs/pipeline-unification/runtime/pruning-contract.md`),
//! purge is a prune operation and must go through `axon-prune`'s plan/execute
//! path rather than calling the vector delete directly — this function now
//! wraps the real `axon_vector::purge` call in a single-step `PrunePlan`
//! driven by `PruneExecutor`, so it gets execution-order handling and the
//! same admin gate every other destructive prune passes through. The
//! URL-matching delete logic itself is unchanged (still `axon-vector`'s
//! scroll + delete), and the wire response is still the same `PurgeResult`
//! shape.

use std::error::Error;
use std::sync::Mutex;

use async_trait::async_trait;
use axon_api::source::ids::{JobId, SourceGenerationId};
use axon_api::source::prune::{PrunePlan, PruneSelector, PruneStep, PruneTargetKind};
use axon_core::config::Config;
use axon_prune::{PruneAuthz, PruneExecutor, PruneTarget, StepExecution};
use uuid::Uuid;

use crate::types::PurgeResult;

#[must_use = "purge returns a Result that should be handled"]
pub async fn purge(
    cfg: &Config,
    target: &str,
    prefix: bool,
    dry_run: bool,
) -> Result<PurgeResult, Box<dyn Error>> {
    let plan = PrunePlan {
        job_id: JobId::new(Uuid::new_v4()),
        selector: PruneSelector::Collection {
            collection: cfg.collection.clone(),
        },
        destructive: !dry_run,
        requires_admin: true,
        estimated: Default::default(),
        steps: vec![PruneStep {
            target: PruneTargetKind::Vector,
            description: format!("purge points matching '{target}' (prefix={prefix})"),
            estimated_deletes: 0,
            vector_selector: None,
            source_id: None,
            generation: None,
        }],
        warnings: Vec::new(),
    };

    let out: Mutex<Option<PurgeResult>> = Mutex::new(None);
    let exec_target = PurgeExecTarget {
        cfg,
        target,
        prefix,
        dry_run,
        out: &out,
    };
    let executor = PruneExecutor::new(exec_target);

    // System-trusted authorization: both callers of this facade — REST
    // `/v1/prune/purge` (router-level `require_admin_scope` layer in
    // `axon-web`'s `admin_routes`) and MCP `prune subaction=purge` (the
    // `CURRENT_PRUNE_AUTHZ` task-local resolved from the caller's real scopes
    // in `axon-mcp`'s `call_tool`) — already enforce `axon:admin` *before*
    // this function is ever reached. Passing `PruneAuthz::admin()` explicitly
    // here (never implicitly defaulted) mirrors the same documented,
    // system-trusted pattern used by the cleanup-debt drain in
    // `crate::source::prune`.
    let authz = PruneAuthz::admin();

    executor
        .execute(&plan, &authz)
        .await
        .map_err(|denied| -> Box<dyn Error> { denied.to_string().into() })?;

    out.into_inner()
        .expect("purge mutex poisoned")
        .ok_or_else(|| -> Box<dyn Error> { "purge executor produced no result".into() })
}

/// [`PruneTarget`] that drives the real `axon-vector` URL-matching delete.
/// Single-step, so `apply()` is called exactly once; the full [`PurgeResult`]
/// (including `sample_urls`/`matched_url_count`, which don't fit
/// [`StepExecution`]'s plain delete count) is stashed in `out` for the caller
/// to read back after `execute()` returns.
struct PurgeExecTarget<'a> {
    cfg: &'a Config,
    target: &'a str,
    prefix: bool,
    dry_run: bool,
    out: &'a Mutex<Option<PurgeResult>>,
}

#[async_trait]
impl PruneTarget for PurgeExecTarget<'_> {
    async fn current_generation(&self, _source_id: Option<&str>) -> Option<SourceGenerationId> {
        // No source/generation scoping for a raw URL/prefix purge target —
        // nothing to fence against.
        None
    }

    async fn apply(&self, _step: &PruneStep) -> Result<StepExecution, String> {
        let result = axon_vector::purge(self.cfg, self.target, self.prefix, self.dry_run)
            .await
            .map_err(|e| e.to_string())?;
        let deleted = result.deleted_points as u64;
        *self.out.lock().expect("purge mutex poisoned") = Some(result);
        Ok(StepExecution::deleted(deleted))
    }
}
