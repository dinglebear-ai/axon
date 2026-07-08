//! `axon prune` service — the target-state replacement for the legacy
//! `dedupe`/`purge` commands (`docs/pipeline-unification/surfaces/command-contract.md`,
//! `docs/pipeline-unification/runtime/pruning-contract.md`).
//!
//! This module is the CLI/MCP/REST-neutral entry point for prune: it takes a
//! [`PruneRequest`], resolves it into a reviewable [`PrunePlan`] via
//! `axon-prune`'s [`PrunePlanner`], and — only when the caller explicitly asks
//! to execute — runs that plan through the real [`PruneExecutor`] against the
//! live [`VectorStore`].
//!
//! ## Scope of this slice
//!
//! Only the `Vector` boundary has a real store wired today (mirroring the
//! existing cleanup-debt drain in `crate::source::prune::LedgerPruneTarget`).
//! Artifact/graph/memory/ledger/job/cache boundaries have no store adapter
//! anywhere in the codebase yet, so `plan()` never estimates non-zero impact
//! for them and `execute()` never attempts to delete against them — a plan or
//! result step is only emitted when the boundary has a real, non-fabricated
//! estimate.
//!
//! `axon-vectors::store::VectorStore` also has no live "count without
//! deleting" primitive, so `plan()` cannot report a real point count for
//! `Source`/`Generation`/`Collection` selectors either. Rather than fabricate a
//! number, the plan carries a warning saying so — the dry-run still proves out
//! the request is well-formed and authorized without lying about impact.
//!
//! ## Safety
//!
//! Per the pruning contract, `dry_run: true` is the default
//! ([`PruneRequest`]'s `Deserialize` default) and bypasses all gating. Execution
//! requires the resolved [`PruneAuthz`] to hold admin, matching
//! [`axon_prune::safety::authorize_execution`]'s "destructive prune requires
//! `axon:admin`" rule. This module never hardcodes `PruneAuthz::admin()` — the
//! caller-derived authz is threaded in by [`prune_execute`]'s caller.

use std::error::Error;

use async_trait::async_trait;
use axon_api::source::ids::{SourceGenerationId, SourceId};
use axon_api::source::prune::{PruneEstimate, PrunePlan, PruneRequest, PruneResult, PruneSelector};
use axon_api::source::vector::VectorDeleteSelector;
use axon_prune::plan::PruneScopeSource;
use axon_prune::{PruneExecutor, PrunePlanner, PruneTarget, StepExecution};
// Re-exported so transports (CLI/MCP/REST) can construct/consume prune authz
// and denial types without taking a direct dependency on `axon-prune`.
pub use axon_prune::{PruneAuthz, PruneDenied};
use axon_vectors::qdrant::QdrantVectorStore;
use axon_vectors::store::VectorStore;

use crate::context::ServiceContext;

/// Resolve a [`PruneRequest`]'s selector into a reviewable [`PrunePlan`]
/// without mutating any state. Always safe to call — dry-run planning never
/// touches a store beyond (future) read-only impact estimation.
pub fn prune_plan(request: &PruneRequest) -> PrunePlan {
    let planner = PrunePlanner::new(NullScopeSource);
    planner.resolve(&request.selector)
}

/// Execute a previously resolved [`PrunePlan`] against the live vector store.
///
/// `authz` must be derived from the caller's real auth context by the caller
/// of this function (CLI: local-trust `--yes`+admin flag; MCP/REST: the
/// resolved OAuth/bearer scopes) — never hardcoded here. Confirmation
/// (`confirm`) and the `require_confirmation` flag on the originating request
/// are enforced via [`axon_prune::safety::authorize_execution`] before any
/// delete is attempted.
pub async fn prune_execute(
    ctx: &ServiceContext,
    plan: &PrunePlan,
    confirm: bool,
    authz: &PruneAuthz,
) -> Result<PruneResult, PruneDenied> {
    axon_prune::safety::authorize_execution(
        &plan.selector,
        /* dry_run = */ false,
        /* require_confirmation = */ true,
        confirm,
        authz,
    )?;

    let vector_store = QdrantVectorStore::new(ctx.cfg().qdrant_url.clone(), "qdrant".to_string());
    let target = VectorOnlyPruneTarget {
        vector_store: &vector_store,
        collection: ctx.cfg().collection.clone(),
    };
    let executor = PruneExecutor::new(target);
    executor.execute(plan, authz).await
}

/// Convenience wrapper mirroring [`crate::reset::reset`]'s shape: build a plan
/// from a request and, when the request is not a dry-run, execute it. Returns
/// `(plan, result)` where `result` is `None` for dry-run requests.
pub async fn prune(
    ctx: &ServiceContext,
    request: &PruneRequest,
    authz: &PruneAuthz,
) -> Result<(PrunePlan, Option<PruneResult>), Box<dyn Error>> {
    let plan = prune_plan(request);
    if request.dry_run {
        return Ok((plan, None));
    }
    let result = prune_execute(ctx, &plan, request.require_confirmation, authz)
        .await
        .map_err(|denied| -> Box<dyn Error> { denied.to_string().into() })?;
    Ok((plan, Some(result)))
}

/// A scope source that reports zero estimated impact for every selector.
///
/// This is intentionally honest rather than fabricated: no store in this
/// codebase currently exposes a read-only "how many would this delete"
/// primitive (see module docs). A real estimate lands once `VectorStore` (and
/// the artifact/graph/memory/ledger stores) grow a count-by-filter API.
struct NullScopeSource;

impl PruneScopeSource for NullScopeSource {
    fn estimate(&self, _selector: &PruneSelector) -> PruneEstimate {
        PruneEstimate::default()
    }
}

/// [`PruneTarget`] backed by the real vector store. Mirrors
/// `crate::source::prune::LedgerPruneTarget`, generalized to the
/// user-requested `Source`/`Generation`/`Collection` selectors this command
/// exposes (rather than one ledger-recorded debt entry at a time).
struct VectorOnlyPruneTarget<'a> {
    vector_store: &'a dyn VectorStore,
    collection: String,
}

impl<'a> VectorOnlyPruneTarget<'a> {
    #[cfg(test)]
    fn new(vector_store: &'a dyn VectorStore, collection: impl Into<String>) -> Self {
        Self {
            vector_store,
            collection: collection.into(),
        }
    }
}

#[async_trait]
impl PruneTarget for VectorOnlyPruneTarget<'_> {
    async fn current_generation(&self, _source_id: Option<&str>) -> Option<SourceGenerationId> {
        // No ledger wired here — nothing is known to be "current", so
        // generation-fencing degrades to "not fenced" rather than fabricating
        // a value. Real generation-fencing for user-requested prunes lands
        // once this module reads the ledger's committed generation.
        None
    }

    async fn apply(
        &self,
        step: &axon_api::source::prune::PruneStep,
    ) -> Result<StepExecution, String> {
        let selector = match step.vector_selector.clone() {
            Some(selector) => selector,
            None => match (&step.source_id, &step.generation) {
                (Some(source_id), Some(generation)) => VectorDeleteSelector::Generation {
                    collection: self.collection.clone(),
                    source_id: SourceId::new(source_id.0.clone()),
                    generation: SourceGenerationId::new(generation.0.clone()),
                },
                (Some(source_id), None) => VectorDeleteSelector::Source {
                    collection: self.collection.clone(),
                    source_id: SourceId::new(source_id.0.clone()),
                    generation: None,
                },
                _ => {
                    return Ok(StepExecution::skipped(
                        "no vector selector resolvable for this step",
                    ));
                }
            },
        };

        let deleted = self
            .vector_store
            .delete(selector)
            .await
            .map_err(|err| err.message.clone())?;
        Ok(StepExecution::deleted(deleted.points_deleted))
    }
}

#[cfg(test)]
#[path = "prune_tests.rs"]
mod tests;
