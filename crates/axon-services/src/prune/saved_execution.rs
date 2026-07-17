use super::*;

pub async fn prune_execute_saved(
    ctx: &ServiceContext,
    plan_id: &str,
    confirm: bool,
    authz: &PruneAuthz,
) -> Result<(PrunePlan, PruneResult, String), Box<dyn Error>> {
    authorize_saved_execution(confirm, authz)?;
    let stored = plan_store::load_plan(ctx, plan_id).await?;
    let existing_receipt = plan_store::load_receipt(ctx, plan_id).await?;
    if existing_receipt
        .as_ref()
        .is_some_and(|receipt| receipt.plan_id != plan_id)
    {
        return Err("prune.receipt_plan_mismatch: refusing to resume another plan".into());
    }
    if let Some(receipt) = existing_receipt.as_ref().filter(|receipt| {
        matches!(
            receipt.status,
            axon_api::source::LifecycleStatus::Completed
                | axon_api::source::LifecycleStatus::CompletedDegraded
        )
    }) {
        return receipt_result(ctx, stored.plan, receipt.clone()).await;
    }
    let expires = chrono::DateTime::parse_from_rfc3339(&stored.expires_at_utc)?;
    if expires < chrono::Utc::now() && existing_receipt.is_none() {
        return Err("prune.plan_expired: create and review a new plan".into());
    }
    verify_inventory(ctx, &stored, existing_receipt.as_ref()).await?;

    let resumed = existing_receipt.is_some();
    let mut receipt = existing_receipt.unwrap_or_else(|| plan_store::StoredPruneReceipt {
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
    if receipt.plan_id != plan_id {
        return Err("prune.receipt_plan_mismatch: refusing to resume another plan".into());
    }
    if resumed {
        receipt.audit_events.push("prune.resume".to_string());
    }
    // Persist the started state before the first destructive boundary. This is
    // what distinguishes an expired-but-resumable execution from an expired
    // plan that never began.
    plan_store::save_receipt(ctx, &receipt).await?;

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
        let chunk_failed = result
            .steps
            .iter()
            .any(|step| step.status == axon_api::source::LifecycleStatus::Failed);
        receipt.steps.extend(result.steps);
        receipt.cleanup_debt_remaining = remaining_debt(&stored.plan, &receipt);
        receipt.audit_events.push(format!(
            "prune.chunk.{}:{:?}",
            if chunk_failed { "failed" } else { "complete" },
            step.target
        ));
        if chunk_failed {
            receipt.status = axon_api::source::LifecycleStatus::Failed;
            return receipt_result(ctx, stored.plan.clone(), receipt).await;
        }
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

async fn verify_inventory(
    ctx: &ServiceContext,
    stored: &plan_store::StoredPrunePlan,
    receipt: Option<&plan_store::StoredPruneReceipt>,
) -> Result<(), Box<dyn Error>> {
    let current = prune_plan_estimated(
        ctx,
        &PruneRequest::dry_run(stored.plan.selector.clone(), stored.reason.clone()),
    )
    .await;
    verify_resumable_plan(stored, &current, receipt)
}

pub(super) fn verify_resumable_plan(
    stored: &plan_store::StoredPrunePlan,
    current: &PrunePlan,
    receipt: Option<&plan_store::StoredPruneReceipt>,
) -> Result<(), Box<dyn Error>> {
    let Some(receipt) = receipt else {
        if plan_store::checksum(current) != stored.inventory_checksum {
            return Err("prune.inventory_changed: create and review a new plan".into());
        }
        return Ok(());
    };

    for planned in &stored.plan.steps {
        let prior = receipt
            .steps
            .iter()
            .find(|step| step.target == planned.target);
        if prior.is_some_and(|step| chunk_completed(receipt, step.target)) {
            if current
                .steps
                .iter()
                .any(|step| step.target == planned.target)
            {
                return Err(format!(
                    "prune.completed_chunk_changed: {:?} target was repopulated",
                    planned.target
                )
                .into());
            }
            continue;
        }

        let current_step = current
            .steps
            .iter()
            .find(|step| step.target == planned.target);
        if prior.is_some_and(|step| step.status == axon_api::source::LifecycleStatus::Failed) {
            // A failed provider call may have applied a prefix before failing.
            // Resume accepts a smaller remainder, but never a broader scope or
            // a count larger than the reviewed destructive bound.
            if let Some(current_step) = current_step
                && (plan_store::step_scope_checksum(current, planned.target)
                    != plan_store::step_scope_checksum(&stored.plan, planned.target)
                    || current_step.estimated_deletes > planned.estimated_deletes)
            {
                return Err("prune.inventory_changed: failed chunk scope expanded".into());
            }
            continue;
        }

        if plan_store::step_checksum(current, planned.target)
            != plan_store::step_checksum(&stored.plan, planned.target)
        {
            return Err("prune.inventory_changed: pending chunk changed since review".into());
        }
    }

    if receipt.steps.iter().any(|step| {
        !stored
            .plan
            .steps
            .iter()
            .any(|planned| planned.target == step.target)
    }) {
        return Err("prune.receipt_shape_invalid: receipt contains an unplanned target".into());
    }
    if receipt.plan_id != stored.plan.job_id.0.to_string() {
        return Err("prune.receipt_plan_mismatch: refusing to resume another plan".into());
    }
    if current.selector != stored.plan.selector {
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
