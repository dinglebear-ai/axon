use axon_api::source::SourceLimits;
use axon_core::config::ProjectionBatchConfig;
use axon_error::{ApiError, ErrorStage};

pub fn effective_limit(caller: Option<u64>, fixed: Option<u64>, server: u64) -> Option<u64> {
    [caller, fixed, Some(server)].into_iter().flatten().min()
}

pub fn validate_input_bytes(input: &str, maximum: usize) -> Result<(), ApiError> {
    validate_bytes("projection.input_too_large", "input", input.len(), maximum)
}

pub fn validate_query_bytes(input: &str, maximum: usize) -> Result<(), ApiError> {
    validate_bytes("projection.query_too_large", "query", input.len(), maximum)
}

pub fn validate_idempotency_key(key: &str, maximum: usize) -> Result<(), ApiError> {
    validate_bytes(
        "projection.idempotency_key_too_large",
        "idempotency key",
        key.len(),
        maximum,
    )
}

pub fn apply_source_limits(
    limits: &mut SourceLimits,
    fixed_pages: Option<u64>,
    policy: &ProjectionBatchConfig,
) {
    limits.max_pages = effective_limit(limits.max_pages, fixed_pages, policy.max_pages);
    limits.max_items = effective_limit(limits.max_items, None, policy.max_manifest_items);
    limits.max_bytes_per_item = effective_limit(
        limits.max_bytes_per_item,
        None,
        policy.max_fetched_bytes_per_item,
    );
    limits.max_total_bytes =
        effective_limit(limits.max_total_bytes, None, policy.max_prepared_bytes);
    limits.max_chunks = effective_limit(limits.max_chunks, None, policy.max_chunks);
}

fn validate_bytes(code: &str, label: &str, actual: usize, maximum: usize) -> Result<(), ApiError> {
    if actual <= maximum {
        return Ok(());
    }
    Err(ApiError::new(
        code,
        ErrorStage::Validation,
        format!("{label} is {actual} bytes; maximum is {maximum} bytes"),
    ))
}

#[cfg(test)]
#[path = "limits_tests.rs"]
mod tests;
