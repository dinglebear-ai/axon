use super::*;
use axon_api::source::enums::LifecycleStatus;
use axon_api::source::prune::{PruneStepResult, PruneTargetKind};
use axon_core::redact::Redactor;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct StoredPrunePlan {
    pub plan: PrunePlan,
    pub reason: String,
    pub inventory_checksum: String,
    pub expires_at_utc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct StoredPruneReceipt {
    pub plan_id: String,
    pub status: LifecycleStatus,
    pub steps: Vec<PruneStepResult>,
    pub cleanup_debt_remaining: u64,
    pub audit_events: Vec<String>,
}

fn root(ctx: &ServiceContext) -> PathBuf {
    ctx.cfg()
        .sqlite_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("prune-control")
}

fn validate_plan_id(plan_id: &str) -> Result<(), Box<dyn Error>> {
    uuid::Uuid::parse_str(plan_id)
        .map(|_| ())
        .map_err(|_| "prune.plan_id_invalid: expected a UUID from `prune plan`".into())
}

fn plan_path(ctx: &ServiceContext, plan_id: &str) -> Result<PathBuf, Box<dyn Error>> {
    validate_plan_id(plan_id)?;
    Ok(root(ctx).join("plans").join(format!("{plan_id}.json")))
}

fn receipt_path(ctx: &ServiceContext, plan_id: &str) -> Result<PathBuf, Box<dyn Error>> {
    validate_plan_id(plan_id)?;
    Ok(root(ctx).join("receipts").join(format!("{plan_id}.json")))
}

pub(super) fn checksum(plan: &PrunePlan) -> String {
    use sha2::{Digest, Sha256};
    let value = serde_json::json!({
        "selector": &plan.selector,
        "estimated": &plan.estimated,
        "steps": &plan.steps,
    });
    let digest = Sha256::digest(value.to_string().as_bytes());
    format!("{digest:x}")
}

pub(super) fn step_checksum(plan: &PrunePlan, target: PruneTargetKind) -> String {
    use sha2::{Digest, Sha256};
    let step = plan.steps.iter().find(|step| step.target == target);
    let value = serde_json::json!({
        "selector": &plan.selector,
        "target": target,
        "step": step,
    });
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

pub(super) fn step_scope_checksum(plan: &PrunePlan, target: PruneTargetKind) -> String {
    use sha2::{Digest, Sha256};
    let mut step = plan
        .steps
        .iter()
        .find(|step| step.target == target)
        .cloned();
    if let Some(step) = &mut step {
        step.estimated_deletes = 0;
    }
    let value = serde_json::json!({
        "selector": &plan.selector,
        "target": target,
        "step": step,
    });
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

pub(super) async fn save_plan(
    ctx: &ServiceContext,
    plan: &PrunePlan,
    reason: &str,
) -> Result<(), Box<dyn Error>> {
    let stored = StoredPrunePlan {
        plan: plan.clone(),
        reason: reason.to_string(),
        inventory_checksum: checksum(plan),
        expires_at_utc: (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339(),
    };
    let path = plan_path(ctx, &plan.job_id.0.to_string())?;
    write_json(&path, &stored).await
}

pub(super) async fn load_plan(
    ctx: &ServiceContext,
    plan_id: &str,
) -> Result<StoredPrunePlan, Box<dyn Error>> {
    let path = plan_path(ctx, plan_id)?;
    read_json(&path).await.map_err(|error| {
        format!("prune.plan_not_found: reviewed plan {plan_id} is unavailable: {error}").into()
    })
}

pub(super) async fn load_receipt(
    ctx: &ServiceContext,
    plan_id: &str,
) -> Result<Option<StoredPruneReceipt>, Box<dyn Error>> {
    let path = receipt_path(ctx, plan_id)?;
    match tokio::fs::read(&path).await {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) async fn save_receipt(
    ctx: &ServiceContext,
    receipt: &StoredPruneReceipt,
) -> Result<String, Box<dyn Error>> {
    let path = receipt_path(ctx, &receipt.plan_id)?;
    write_json(&path, receipt).await?;
    Ok(path.display().to_string())
}

async fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, Box<dyn Error>> {
    Ok(serde_json::from_slice(&tokio::fs::read(path).await?)?)
}

async fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn Error>> {
    let parent = path.parent().ok_or("prune control path has no parent")?;
    let value = serde_json::to_value(value)?;
    let (value, _) = axon_core::redact::DefaultRedactor::new().redact_json(
        value,
        &axon_core::redact::RedactionContext::artifact_metadata(),
    );
    let file_name = path
        .file_name()
        .ok_or("prune control path has no file name")?;
    axon_core::artifacts::atomic_write_under(
        parent,
        file_name,
        &serde_json::to_vec_pretty(&value)?,
    )
    .await?;
    Ok(())
}
