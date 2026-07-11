//! Dedupe near-identical chunks within a Qdrant collection.
//!
//! Per the pruning contract (`docs/pipeline-unification/runtime/pruning-contract.md`
//! §"Dedupe"), dedupe is a prune operation with a non-source selector and must
//! go through `axon-prune`'s plan/execute path. This facade wraps the real
//! `axon_vector::dedupe_payload` scan-and-delete call in a single-step
//! `PrunePlan` driven by `PruneExecutor` so it gets the same admin gate and
//! execution accounting every other destructive prune passes through. The
//! duplicate-detection/deletion logic itself is unchanged (still
//! `axon-vector`'s two-pass scroll), and the wire response is still the same
//! `DedupeResult` shape.

use std::error::Error;
use std::sync::Mutex;

use async_trait::async_trait;
use axon_api::source::ids::{JobId, SourceGenerationId};
use axon_api::source::prune::{PrunePlan, PruneSelector, PruneStep, PruneTargetKind};
use axon_core::config::Config;
use axon_prune::{PruneAuthz, PruneExecutor, PruneTarget, StepExecution};
use axon_vector::ops::qdrant::dedupe_payload;
use uuid::Uuid;

use crate::events::{LogLevel, ServiceEvent, emit};
use crate::types::DedupeResult;

#[must_use = "dedupe returns a Result that should be handled"]
pub async fn dedupe(
    cfg: &Config,
    tx: Option<tokio::sync::mpsc::Sender<ServiceEvent>>,
) -> Result<DedupeResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "starting dedupe".to_string(),
        },
    )
    .await;

    let plan = PrunePlan {
        job_id: JobId::new(Uuid::new_v4()),
        selector: PruneSelector::Collection {
            collection: cfg.collection.clone(),
        },
        destructive: true,
        requires_admin: true,
        estimated: Default::default(),
        steps: vec![PruneStep {
            target: PruneTargetKind::Vector,
            description: "dedupe near-identical chunks".to_string(),
            estimated_deletes: 0,
            vector_selector: None,
            source_id: None,
            generation: None,
            graph_stable_keys: None,
            graph_edge_ids: None,
            memory_ids: None,
        }],
        warnings: Vec::new(),
    };

    let out: Mutex<Option<(usize, usize)>> = Mutex::new(None);
    let executor = PruneExecutor::new(DedupeExecTarget { cfg, out: &out });

    // System-trusted authorization: both callers of this facade — REST
    // `/v1/prune/dedupe` (router-level `require_admin_scope` layer in
    // `axon-web`'s `admin_routes`) and MCP `prune subaction=dedupe` (the
    // `CURRENT_PRUNE_AUTHZ` task-local resolved from the caller's real scopes
    // in `axon-mcp`'s `call_tool`) — already enforce `axon:admin` *before*
    // this function is ever reached. Passing `PruneAuthz::admin()` explicitly
    // here (never implicitly defaulted) mirrors the same documented,
    // system-trusted pattern used by the cleanup-debt drain in
    // `crate::source::prune`.
    let authz = PruneAuthz::admin();

    let outcome = executor.execute(&plan, &authz).await;

    match outcome {
        Ok(_) => {
            let (duplicate_groups, deleted) = out
                .into_inner()
                .expect("dedupe mutex poisoned")
                .unwrap_or((0, 0));
            emit(
                &tx,
                ServiceEvent::Log {
                    level: LogLevel::Info,
                    message: format!(
                        "completed dedupe: {duplicate_groups} groups, {deleted} deleted"
                    ),
                },
            )
            .await;
            Ok(DedupeResult {
                completed: true,
                duplicate_groups,
                deleted,
            })
        }
        Err(denied) => {
            let msg = format!("dedupe failed: {denied}");
            emit(
                &tx,
                ServiceEvent::Log {
                    level: LogLevel::Error,
                    message: msg.clone(),
                },
            )
            .await;
            Err(msg.into())
        }
    }
}

/// [`PruneTarget`] that drives the real `axon-vector` dedupe scan+delete.
/// Single-step, so `apply()` is called exactly once; `duplicate_groups`
/// (which doesn't fit [`StepExecution`]'s plain delete count) is stashed in
/// `out` for the caller to read back after `execute()` returns.
struct DedupeExecTarget<'a> {
    cfg: &'a Config,
    out: &'a Mutex<Option<(usize, usize)>>,
}

#[async_trait]
impl PruneTarget for DedupeExecTarget<'_> {
    async fn current_generation(&self, _source_id: Option<&str>) -> Option<SourceGenerationId> {
        // Dedupe is collection-wide, not source/generation scoped — nothing
        // to fence against.
        None
    }

    async fn apply(&self, _step: &PruneStep) -> Result<StepExecution, String> {
        // `dedupe_payload` returns a `Box<dyn Error + Send + Sync>`; converted
        // to a plain `String` immediately so nothing `!Send` crosses an
        // `.await` boundary in the caller.
        let value = dedupe_payload(self.cfg)
            .await
            .map_err(|e| format!("dedupe failed: {e}"))?;
        let duplicate_groups = value
            .get("duplicate_groups")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize;
        let deleted = value
            .get("deleted")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize;
        *self.out.lock().expect("dedupe mutex poisoned") = Some((duplicate_groups, deleted));
        Ok(StepExecution::deleted(deleted as u64))
    }
}
