use axon_api::source::*;
use axon_error::{ApiError, ErrorStage};
use uuid::Uuid;

use super::limits::{
    apply_source_limits, validate_idempotency_key, validate_input_bytes, validate_query_bytes,
};
use super::{
    PreparedCodeSearchItem, PreparedSourceItem, ProjectionBatchPolicy, ProjectionPreflight,
    SourceAccessPolicy,
};
use crate::source::{authorize::SourceAccessDecision, routing};

pub fn preflight_source_batch(
    operation: ProjectionOperation,
    requests: Vec<SourceRequest>,
    auth: Option<&AuthSnapshot>,
    policy: &ProjectionBatchPolicy,
    access: &SourceAccessPolicy,
) -> Result<ProjectionPreflight<PreparedSourceItem>, ApiError> {
    validate_count(requests.len(), policy.max_inputs)?;
    let mut total = 0usize;
    let mut prepared = Vec::with_capacity(requests.len());
    for (index, mut request) in requests.into_iter().enumerate() {
        validate_input_bytes(&request.source, policy.max_input_bytes)?;
        total = total.checked_add(request.source.len()).ok_or_else(|| {
            preflight_error(
                "projection.aggregate_input_overflow",
                "input byte count overflow",
            )
        })?;
        if let Some(key) = &request.idempotency_key {
            validate_idempotency_key(key, policy.max_idempotency_key_bytes)?;
        }
        apply_source_limits(
            &mut request.limits,
            (operation == ProjectionOperation::Scrape).then_some(1),
            policy,
        );
        let routed = routing::resolve_source_route_for_access(
            &request,
            auth,
            access.operator_allows_tool_execution,
        )?;
        let decision = SourceAccessDecision::evaluate(
            &routed.route,
            &request.source,
            routed.kind,
            auth,
            access.affinity,
            access.allowed_roots.as_deref(),
        )?;
        prepared.push(PreparedSourceItem {
            index,
            request,
            kind: routed.kind,
            route: routed.route,
            required_scope: decision.required_scope,
        });
    }
    validate_aggregate(total, policy.max_aggregate_input_bytes)?;
    validate_request_size(
        &prepared
            .iter()
            .map(|item| &item.request)
            .collect::<Vec<_>>(),
        policy.max_request_bytes,
    )?;
    Ok(ProjectionPreflight {
        batch_id: BatchId::new(Uuid::new_v4()),
        items: prepared,
        aggregate_input_bytes: total,
    })
}

pub fn preflight_code_search_batch(
    plans: Vec<CodeSearchPlan>,
    policy: &ProjectionBatchPolicy,
) -> Result<ProjectionPreflight<PreparedCodeSearchItem>, ApiError> {
    validate_count(plans.len(), policy.max_inputs)?;
    let mut total = 0usize;
    let mut prepared = Vec::with_capacity(plans.len());
    for (index, mut plan) in plans.into_iter().enumerate() {
        validate_query_bytes(&plan.query, policy.max_query_bytes)?;
        total = total.checked_add(plan.query.len()).ok_or_else(|| {
            preflight_error(
                "projection.aggregate_input_overflow",
                "query byte count overflow",
            )
        })?;
        if plan.offset >= policy.max_query_window {
            return Err(preflight_error(
                "projection.query_window_exceeded",
                "query offset exceeds the configured search window",
            ));
        }
        plan.limit = plan
            .limit
            .min(policy.max_query_window.saturating_sub(plan.offset));
        prepared.push(PreparedCodeSearchItem { index, plan });
    }
    validate_aggregate(total, policy.max_aggregate_input_bytes)?;
    validate_request_size(
        &prepared.iter().map(|item| &item.plan).collect::<Vec<_>>(),
        policy.max_request_bytes,
    )?;
    Ok(ProjectionPreflight {
        batch_id: BatchId::new(Uuid::new_v4()),
        items: prepared,
        aggregate_input_bytes: total,
    })
}

fn validate_request_size<T: serde::Serialize>(request: &T, maximum: usize) -> Result<(), ApiError> {
    let actual = serde_json::to_vec(request)
        .map_err(|error| preflight_error("projection.request_encoding_failed", error.to_string()))?
        .len();
    if actual <= maximum {
        return Ok(());
    }
    Err(preflight_error(
        "projection.request_too_large",
        format!("normalized request is {actual} bytes; maximum is {maximum} bytes"),
    ))
}

fn validate_count(actual: usize, maximum: usize) -> Result<(), ApiError> {
    if actual > 0 && actual <= maximum {
        return Ok(());
    }
    Err(preflight_error(
        "projection.input_count_invalid",
        format!("request has {actual} inputs; expected 1..={maximum}"),
    ))
}

fn validate_aggregate(actual: usize, maximum: usize) -> Result<(), ApiError> {
    if actual <= maximum {
        return Ok(());
    }
    Err(preflight_error(
        "projection.aggregate_input_too_large",
        format!("aggregate input is {actual} bytes; maximum is {maximum} bytes"),
    ))
}

fn preflight_error(code: &str, message: impl Into<String>) -> ApiError {
    ApiError::new(code, ErrorStage::Validation, message)
}

#[cfg(test)]
#[path = "preflight_tests.rs"]
mod tests;
