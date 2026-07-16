use super::*;

pub async fn prune_execute_saved(
    ctx: &ServiceContext,
    plan_id: &str,
    confirm: bool,
    authz: &PruneAuthz,
) -> Result<(PrunePlan, PruneResult, String), Box<dyn Error>> {
    authorize_saved_execution(confirm, authz)?;
    let stored = plan_store::load_plan(ctx, plan_id).await?;
    let expires = chrono::DateTime::parse_from_rfc3339(&stored.expires_at_utc)?;
    if expires < chrono::Utc::now() {
        return Err("prune.plan_expired: create and review a new plan".into());
    }
    let completed = completed_receipt(ctx, plan_id).await?;
    if let Some(receipt) = completed {
        return receipt_result(ctx, stored.plan, receipt).await;
    }
    verify_inventory(ctx, &stored).await?;

    let mut receipt = plan_store::load_receipt(ctx, plan_id)
        .await?
        .unwrap_or_else(|| plan_store::StoredPruneReceipt {
            plan_id: plan_id.to_string(),
            status: axon_api::source::LifecycleStatus::Running,
            steps: Vec::new(),
            cleanup_debt_remaining: 0,
            audit_events: vec![
                "prune.plan".to_string(),
                "prune.confirm".to_string(),
                "prune.execute".to_string(),
            ],
        });
    if !receipt.steps.is_empty() {
        receipt.audit_events.push("prune.resume".to_string());
    }

    for step in &stored.plan.steps {
        if chunk_completed(&receipt, step.target) {
            continue;
        }
        receipt.steps.retain(|prior| prior.target != step.target);
        let mut chunk_plan = stored.plan.clone();
        chunk_plan.steps = vec![step.clone()];
        let result = prune_execute(ctx, &chunk_plan, true, authz)
            .await
            .map_err(|denied| -> Box<dyn Error> { denied.to_string().into() })?;
        receipt.steps.extend(result.steps);
        receipt.cleanup_debt_remaining = remaining_debt(&stored.plan, &receipt);
        receipt
            .audit_events
            .push(format!("prune.chunk.complete:{:?}", step.target));
        plan_store::save_receipt(ctx, &receipt).await?;
    }
    receipt.status = if receipt.cleanup_debt_remaining > 0 {
        axon_api::source::LifecycleStatus::CompletedDegraded
    } else {
        axon_api::source::LifecycleStatus::Completed
    };
    receipt.audit_events.push("prune.complete".to_string());
    receipt_result(ctx, stored.plan, receipt).await
}

fn authorize_saved_execution(confirm: bool, authz: &PruneAuthz) -> Result<(), Box<dyn Error>> {
    if !confirm {
        return Err("prune.confirmation_required: pass --confirm".into());
    }
    if !authz.is_admin {
        return Err("prune.admin_required: destructive prune requires axon:admin".into());
    }
    Ok(())
}

async fn completed_receipt(
    ctx: &ServiceContext,
    plan_id: &str,
) -> Result<Option<plan_store::StoredPruneReceipt>, Box<dyn Error>> {
    Ok(plan_store::load_receipt(ctx, plan_id)
        .await?
        .filter(|receipt| {
            matches!(
                receipt.status,
                axon_api::source::LifecycleStatus::Completed
                    | axon_api::source::LifecycleStatus::CompletedDegraded
            )
        }))
}

async fn verify_inventory(
    ctx: &ServiceContext,
    stored: &plan_store::StoredPrunePlan,
) -> Result<(), Box<dyn Error>> {
    let current = prune_plan_estimated(
        ctx,
        &PruneRequest::dry_run(stored.plan.selector.clone(), stored.reason.clone()),
    )
    .await;
    if plan_store::checksum(&current) != stored.inventory_checksum {
        return Err("prune.inventory_changed: create and review a new plan".into());
    }
    Ok(())
}

fn chunk_completed(
    receipt: &plan_store::StoredPruneReceipt,
    target: axon_api::source::prune::PruneTargetKind,
) -> bool {
    receipt.steps.iter().any(|done| {
        done.target == target
            && matches!(
                done.status,
                axon_api::source::LifecycleStatus::Completed
                    | axon_api::source::LifecycleStatus::Skipped
            )
    })
}

fn remaining_debt(plan: &PrunePlan, receipt: &plan_store::StoredPruneReceipt) -> u64 {
    receipt
        .steps
        .iter()
        .filter(|chunk| chunk.status == axon_api::source::LifecycleStatus::Failed)
        .filter_map(|chunk| {
            plan.steps
                .iter()
                .find(|planned| planned.target == chunk.target)
                .map(|planned| planned.estimated_deletes)
        })
        .sum()
}

async fn receipt_result(
    ctx: &ServiceContext,
    plan: PrunePlan,
    receipt: plan_store::StoredPruneReceipt,
) -> Result<(PrunePlan, PruneResult, String), Box<dyn Error>> {
    let deleted_counts = axon_prune::receipt::counts_from_steps(&receipt.steps);
    let path = plan_store::save_receipt(ctx, &receipt).await?;
    let result = PruneResult {
        job_id: plan.job_id,
        status: receipt.status,
        steps: receipt.steps,
        deleted_counts,
        cleanup_debt_remaining: receipt.cleanup_debt_remaining,
    };
    Ok((plan, result, path))
}
