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
use axon_api::source::common::SourceWarning;
use axon_api::source::enums::Severity;
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
///
/// This is the zero-dependency variant (no `ServiceContext`/ledger access) —
/// it always reports [`NullScopeSource`]'s honest zero. Prefer
/// [`prune_plan_estimated`] wherever a `ServiceContext` is available; it
/// reports real ledger-backed counts for `Source`/`Generation` selectors.
pub fn prune_plan(request: &PruneRequest) -> PrunePlan {
    let planner = PrunePlanner::new(NullScopeSource);
    let mut plan = planner.resolve(&request.selector);
    warn_if_unsupported(&mut plan);
    plan
}

/// Real-count variant of [`prune_plan`]: reads `ctx`'s
/// [`axon_ledger::store::LedgerStore`] for a genuine, non-fabricated impact
/// estimate before resolving the plan.
///
/// Only `Source`/`Generation` selectors are sizeable from the ledger today —
/// `vector_points` is reported from the committed manifest's item count (a
/// real, ledger-backed proxy for chunk count; `VectorStore` still has no
/// count-without-deleting primitive, see module docs) and `ledger_generations`
/// reflects whether a committed generation/manifest was actually found.
/// Other selector shapes (`Collection`, `CleanupDebt`, `Artifact`, `Graph`,
/// `Memory`, `JobRetention`, `Cache`) still resolve to a zero estimate for the
/// same honest-zero reason `NullScopeSource` documents — the ledger has
/// nothing to size for them.
pub async fn prune_plan_estimated(ctx: &ServiceContext, request: &PruneRequest) -> PrunePlan {
    let estimate = match ctx.target_local_source_runtime() {
        Some(runtime) => estimate_from_ledger(runtime.ledger.as_ref(), &request.selector).await,
        // No target-local ledger wired for this `ServiceContext` (e.g. a
        // pure-vector `ServiceContext::new`) — honest zero, same rationale as
        // `NullScopeSource`.
        None => PruneEstimate::default(),
    };
    let planner = PrunePlanner::new(PrefetchedScopeSource(estimate));
    let mut plan = planner.resolve(&request.selector);
    warn_if_unsupported(&mut plan);
    plan
}

/// Compute a real `PruneEstimate` for `selector` from ledger data. See
/// [`prune_plan_estimated`] for what is and is not sizeable this way.
async fn estimate_from_ledger(
    ledger: &dyn axon_ledger::store::LedgerStore,
    selector: &PruneSelector,
) -> PruneEstimate {
    match selector {
        PruneSelector::Source { source_id } => {
            match ledger.committed_generation(source_id.clone()).await {
                Ok(Some(generation)) => {
                    manifest_estimate(ledger, source_id.clone(), generation).await
                }
                _ => PruneEstimate::default(),
            }
        }
        PruneSelector::Generation {
            source_id,
            generation,
        } => manifest_estimate(ledger, source_id.clone(), generation.clone()).await,
        // Collection/CleanupDebt/Artifact/Graph/Memory/JobRetention/Cache
        // selectors don't name a ledger-sizeable source+generation — honest
        // zero, same as `NullScopeSource`.
        _ => PruneEstimate::default(),
    }
}

async fn manifest_estimate(
    ledger: &dyn axon_ledger::store::LedgerStore,
    source_id: SourceId,
    generation: SourceGenerationId,
) -> PruneEstimate {
    match ledger.get_manifest(source_id, generation).await {
        Ok(Some(manifest)) => PruneEstimate {
            vector_points: manifest.items.len() as u64,
            ledger_generations: 1,
            ..Default::default()
        },
        _ => PruneEstimate::default(),
    }
}

/// Guidance for selectors this vector-only prune target cannot execute a delete
/// against today. `Source`/`Generation` are wired; everything else (Collection,
/// CleanupDebt, Artifact, Graph, Memory, JobRetention, Cache) has no store
/// adapter, so executing it would silently delete nothing. We refuse loudly and
/// warn on the plan instead — collection-wide wipes belong to `axon reset`.
fn unsupported_selector_guidance(selector: &PruneSelector) -> Option<String> {
    match selector {
        PruneSelector::Source { .. } | PruneSelector::Generation { .. } => None,
        PruneSelector::Collection { .. } => Some(
            "collection-wide prune is not implemented; use `axon reset` to wipe an entire \
             collection, or prune by `--source`/`--generation`"
                .to_string(),
        ),
        _ => Some(
            "this selector's boundary has no delete adapter yet; only `--source` and \
             `--generation` prunes are wired to a store today"
                .to_string(),
        ),
    }
}

/// Push a warning onto the plan when its selector cannot be executed, so the
/// dry-run (and the plan returned alongside an execute) never reads as a clean
/// no-op success.
fn warn_if_unsupported(plan: &mut PrunePlan) {
    if let Some(guidance) = unsupported_selector_guidance(&plan.selector) {
        plan.warnings.push(SourceWarning {
            code: "prune.selector_unsupported".to_string(),
            severity: Severity::Warning,
            message: guidance,
            source_item_key: None,
            retryable: false,
        });
    }
}

/// A [`PruneScopeSource`] that always returns a single, precomputed estimate
/// regardless of the selector passed to `estimate()`. Valid because a caller
/// only ever resolves one selector per [`PrunePlanner::resolve`] call — the
/// async ledger read happens once, up front, in [`prune_plan_estimated`].
struct PrefetchedScopeSource(PruneEstimate);

impl PruneScopeSource for PrefetchedScopeSource {
    fn estimate(&self, _selector: &PruneSelector) -> PruneEstimate {
        self.0.clone()
    }
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

    // Refuse selectors with no wired delete adapter rather than running an empty
    // plan and reporting a no-op as success (the vector-only target can only
    // execute `Source`/`Generation`).
    if let Some(guidance) = unsupported_selector_guidance(&plan.selector) {
        return Err(PruneDenied::Unsupported {
            selector: format!("{:?}", plan.selector),
            guidance,
        });
    }

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
    let plan = prune_plan_estimated(ctx, request).await;
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
