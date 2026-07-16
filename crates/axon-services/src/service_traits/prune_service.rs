//! `PruneService` — reviewable destructive cleanup (plan/execute/cleanup debt).
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §PruneService. Execution consumes a persisted reviewed plan id and requires
//! caller-derived authorization; no removed purge/dedupe public operation is
//! represented here.

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::{PruneExecuteRequest, PrunePlan, PruneRequest, PruneResult};
use axon_prune::PruneAuthz;

use crate::context::ServiceContext;
use crate::service_traits::not_implemented;

/// Deferred per the module doc comment: no `CleanupDebtRequest` DTO and no
/// backing free function exist yet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CleanupDebtRequest {
    pub source_id: Option<String>,
    pub dry_run: bool,
}

#[async_trait]
pub trait PruneService: Send + Sync {
    async fn plan(&self, request: PruneRequest) -> anyhow::Result<PrunePlan>;
    async fn execute(
        &self,
        request: PruneExecuteRequest,
        authz: &PruneAuthz,
    ) -> anyhow::Result<PruneResult>;
    async fn cleanup_debt(
        &self,
        request: CleanupDebtRequest,
    ) -> anyhow::Result<axon_api::source::stage::CleanupDebtResult>;
}

pub struct PruneServiceImpl {
    ctx: Arc<ServiceContext>,
}

impl PruneServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl PruneService for PruneServiceImpl {
    async fn plan(&self, request: PruneRequest) -> anyhow::Result<PrunePlan> {
        let (plan, _) = crate::prune::prune(&self.ctx, &request, &PruneAuthz::anonymous())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(plan)
    }

    async fn execute(
        &self,
        request: PruneExecuteRequest,
        authz: &PruneAuthz,
    ) -> anyhow::Result<PruneResult> {
        crate::prune::prune_execute_saved(
            &self.ctx,
            &request.plan_id.0.to_string(),
            request.confirm,
            authz,
        )
        .await
        .map(|(_, result, _)| result)
        .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn cleanup_debt(
        &self,
        _request: CleanupDebtRequest,
    ) -> anyhow::Result<axon_api::source::stage::CleanupDebtResult> {
        Err(not_implemented("PruneService::cleanup_debt"))
    }
}

/// Deterministic in-memory fake covering every `PruneService` method.
#[derive(Default)]
pub struct FakePruneService;

impl FakePruneService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PruneService for FakePruneService {
    async fn plan(&self, request: PruneRequest) -> anyhow::Result<PrunePlan> {
        Ok(PrunePlan {
            job_id: axon_api::source::JobId::new(uuid::Uuid::new_v4()),
            selector: request.selector,
            destructive: !request.dry_run,
            requires_admin: true,
            estimated: axon_api::source::PruneEstimate::default(),
            steps: Vec::new(),
            warnings: Vec::new(),
        })
    }

    async fn execute(
        &self,
        request: PruneExecuteRequest,
        authz: &PruneAuthz,
    ) -> anyhow::Result<PruneResult> {
        if !request.confirm {
            anyhow::bail!("prune execute requires confirm=true");
        }
        if !authz.is_admin {
            anyhow::bail!("prune execute requires axon:admin");
        }
        Ok(PruneResult {
            job_id: request.plan_id,
            status: axon_api::source::LifecycleStatus::Completed,
            steps: Vec::new(),
            deleted_counts: axon_api::source::PruneCounts::default(),
            cleanup_debt_remaining: 0,
        })
    }

    async fn cleanup_debt(
        &self,
        request: CleanupDebtRequest,
    ) -> anyhow::Result<axon_api::source::stage::CleanupDebtResult> {
        let _ = request.dry_run;
        let now = axon_api::source::Timestamp::from(chrono::Utc::now());
        Ok(axon_api::source::stage::CleanupDebtResult {
            header: axon_api::source::stage::StageResultHeader {
                job_id: axon_api::source::JobId::new(uuid::Uuid::new_v4()),
                stage_id: axon_api::source::StageId::new(uuid::Uuid::new_v4()),
                phase: axon_api::source::PipelinePhase::Queued,
                status: axon_api::source::LifecycleStatus::Completed,
                started_at: now,
                completed_at: None,
                counts: axon_api::source::stage::StageCounts {
                    items_total: None,
                    items_done: 0,
                    documents_total: None,
                    documents_done: 0,
                    chunks_total: None,
                    chunks_done: 0,
                    bytes_total: None,
                    bytes_done: 0,
                },
                warnings: Vec::new(),
                error: None,
            },
            debt_id: axon_api::source::CleanupDebtId::new(uuid_like_id()),
            kind: axon_api::source::CleanupDebtKind::VectorDelete,
            status: axon_api::source::LifecycleStatus::Completed,
            items_attempted: 0,
            items_cleaned: 0,
            next_retry_at: None,
        })
    }
}

fn uuid_like_id() -> String {
    format!("debt-{}", uuid::Uuid::new_v4())
}

#[cfg(test)]
#[path = "prune_service_tests.rs"]
mod tests;
