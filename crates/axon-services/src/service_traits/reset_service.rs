//! `ResetService` — clean-slate destructive reset (plan/execute).
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §ResetService. `crate::reset::reset` is the only existing free function
//! and is a single-shot dry-run-or-execute decided by `Config` flags
//! (`cfg.reset_dry_run`/`cfg.yes`), not a plan-id-issued/confirm-by-id
//! two-phase flow. There is no persisted plan keyed by `ResetId` and no
//! `ResetConfirmation` validation — `plan`/`execute` both call `reset(cfg)`
//! with a forced dry-run flag as a pragmatic stopgap (see the wiring plan's
//! risk notes); `execute` re-derives the whole plan from scratch rather than
//! executing a previously issued plan. This is a known limitation, not a
//! faithful two-phase contract implementation.

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::reset::{ResetPlan, ResetResult};

use crate::context::ServiceContext;

#[async_trait]
pub trait ResetService: Send + Sync {
    async fn plan(&self) -> anyhow::Result<ResetPlan>;
    async fn execute(&self) -> anyhow::Result<ResetResult>;
}

pub struct ResetServiceImpl {
    ctx: Arc<ServiceContext>,
}

impl ResetServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ResetService for ResetServiceImpl {
    async fn plan(&self) -> anyhow::Result<ResetPlan> {
        let mut cfg = (*self.ctx.cfg()).clone();
        cfg.reset_dry_run = true;
        let result = crate::reset::reset(&cfg)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(result.reset_plan)
    }

    async fn execute(&self) -> anyhow::Result<ResetResult> {
        let mut cfg = (*self.ctx.cfg()).clone();
        cfg.reset_dry_run = false;
        cfg.yes = true;
        crate::reset::reset(&cfg)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

/// Deterministic in-memory fake covering `ResetService::{plan,execute}`.
#[derive(Default)]
pub struct FakeResetService;

impl FakeResetService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ResetService for FakeResetService {
    async fn plan(&self) -> anyhow::Result<ResetPlan> {
        Ok(ResetPlan {
            plan_id: "fake-reset-plan-1".to_string(),
            reset_id: "fake-reset-1".to_string(),
            stores: vec!["jobs".to_string()],
            estimates: axon_api::reset::ResetEstimate::default(),
            inventory_checksum: "fake-checksum".to_string(),
            config_snapshot_id: "fake-config-snapshot".to_string(),
            auth_snapshot_id: "fake-auth-snapshot".to_string(),
            confirmation_text: "RESET".to_string(),
            receipt_path: None,
            expires_at_utc: chrono::Utc::now().to_rfc3339(),
            blockers: Vec::new(),
        })
    }

    async fn execute(&self) -> anyhow::Result<ResetResult> {
        let plan = self.plan().await?;
        Ok(ResetResult {
            plan_id: plan.plan_id.clone(),
            reset_id: plan.reset_id.clone(),
            stores: plan.stores.clone(),
            dry_run: false,
            plan: Vec::new(),
            estimates: axon_api::reset::ResetEstimate::default(),
            execution_state: axon_api::reset::ResetExecutionState::Completed,
            inventory_checksum: plan.inventory_checksum.clone(),
            config_snapshot_id: plan.config_snapshot_id.clone(),
            auth_snapshot_id: plan.auth_snapshot_id.clone(),
            confirmation_text: plan.confirmation_text.clone(),
            plan_expires_at_utc: plan.expires_at_utc.clone(),
            blockers: plan.blockers.clone(),
            chunks: Vec::new(),
            audit_events: Vec::new(),
            deleted: axon_api::reset::ResetDeleted::default(),
            created: axon_api::reset::ResetCreated::default(),
            receipt_path: None,
            warnings: Vec::new(),
            reset_plan: plan,
        })
    }
}

#[cfg(test)]
#[path = "reset_service_tests.rs"]
mod tests;
