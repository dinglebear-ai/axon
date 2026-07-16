//! `ResetService` — clean-slate destructive reset (plan/execute).
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §ResetService. Planning persists a reusable plan; execution requires that
//! reviewed plan id, explicit confirmation, and caller-derived admin auth.

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::reset::{ResetPlan, ResetResult};

use crate::context::ServiceContext;
use crate::reset::ResetAuthz;

#[async_trait]
pub trait ResetService: Send + Sync {
    async fn plan(&self) -> anyhow::Result<ResetPlan>;
    async fn execute(
        &self,
        plan_id: &str,
        confirmed: bool,
        authz: &ResetAuthz,
    ) -> anyhow::Result<ResetResult>;
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

    async fn execute(
        &self,
        plan_id: &str,
        confirmed: bool,
        authz: &ResetAuthz,
    ) -> anyhow::Result<ResetResult> {
        if !confirmed {
            anyhow::bail!("reset.confirmation_required");
        }
        let mut cfg = (*self.ctx.cfg()).clone();
        cfg.reset_dry_run = false;
        cfg.yes = true;
        cfg.reset_plan_id = Some(plan_id.to_string());
        crate::reset::reset_with_authz(&cfg, authz)
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

    async fn execute(
        &self,
        plan_id: &str,
        confirmed: bool,
        authz: &ResetAuthz,
    ) -> anyhow::Result<ResetResult> {
        if !confirmed || !authz.is_admin {
            anyhow::bail!("reset execution denied");
        }
        let plan = self.plan().await?;
        if plan.plan_id != plan_id {
            anyhow::bail!("reset.plan_not_found");
        }
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
