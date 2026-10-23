//! Authoritative ledger visibility for foreground, nonhistorical vector reads.
use std::collections::HashMap;

use axon_api::source::*;
use axon_ledger::store::LedgerStore;
use axon_vectors::payload::generation_payload_i64;

/// Cache contains only this search's candidate sources, not process-global state.
/// Do not use citation/born epoch: carried points preserve origin provenance.
pub(super) async fn reconcile(
    ledger: &dyn LedgerStore,
    request: &VectorSearchRequest,
    mut result: VectorSearchResult,
) -> Result<VectorSearchResult, ApiError> {
    // An explicit generation is an intentional historical lookup; that entry
    // point owns its historical authorization/receipt policy. Plain query and
    // ask construct generation=None and always take the authoritative fence.
    if request.generation.is_some() {
        return Ok(result);
    }
    let mut epochs: HashMap<String, Option<i64>> = HashMap::new();
    let mut admitted = Vec::with_capacity(result.results.len());
    let original_count = result.results.len();
    for hit in result.results {
        let source = hit
            .payload
            .get("source_id")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| invalid_hit(&hit.point_id.0, "source_id"))?;
        if hit.source_id.as_ref().is_some_and(|id| id.0 != source) {
            return Err(invalid_hit(&hit.point_id.0, "source_id identity mismatch"));
        }
        let epoch = match epochs.get(source) {
            Some(epoch) => *epoch,
            None => {
                let current = ledger.committed_generation(SourceId::new(source)).await
                    .map_err(|cause| ApiError::new(
                        "retrieval.publication_fence_unavailable", ErrorStage::Retrieving,
                        "cannot verify current source publication; no query results returned; restore ledger availability before retrying",
                    ).with_context("source_id", source.to_string())
                     .with_context("ledger_error_code", cause.code.to_string()))?;
                let epoch = current
                    .map(|generation| generation_payload_i64(&generation, "committed_generation"))
                    .transpose()?;
                epochs.insert(source.to_string(), epoch);
                epoch
            }
        };
        let committed = hit
            .payload
            .get("committed_generation")
            .and_then(|value| value.as_i64());
        // Missing authority or malformed/staged epoch is never an invitation
        // to return a provider point from an unknown or future generation.
        if epoch.is_some() && committed == epoch {
            admitted.push(hit);
        }
    }
    let rejected = original_count - admitted.len();
    result.results = admitted;
    if rejected > 0 {
        tracing::warn!(
            stage = "retrieving",
            operation = "publication_fence",
            candidates_rejected = rejected,
            candidates_admitted = result.results.len(),
            "noncurrent vector candidates excluded using authoritative ledger epochs; result window may underfill until retirement cleanup completes"
        );
        result.warnings.push(SourceWarning {
            code: "retrieval.noncurrent_candidates_excluded".to_string(),
            severity: Severity::Warning,
            message: format!("excluded {rejected} noncurrent or unauthoritative vector candidates; result window may underfill; inspect source publication and retirement cleanup debt"),
            source_item_key: None,
            retryable: true,
        });
        result.metadata.insert(
            "publication_fence_rejected".to_string(),
            serde_json::json!(rejected),
        );
    }
    Ok(result)
}

fn invalid_hit(point: &str, field: &str) -> ApiError {
    ApiError::new("retrieval.invalid_publication_identity", ErrorStage::Retrieving,
        "vector candidate has invalid source publication identity; no query results returned; repair vector payload before retrying")
        .with_context("point_id", point.to_string())
        .with_context("field", field.to_string())
}

#[cfg(test)]
#[path = "publication_fence_tests.rs"]
mod tests;
