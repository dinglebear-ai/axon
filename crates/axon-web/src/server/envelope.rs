//! Shared REST success envelope (U2-18).
//!
//! `axon_api::source::SuccessEnvelope<T>` is the contracted shape for every
//! non-stream, non-byte REST success response
//! (`docs/pipeline-unification/surfaces/rest-contract.md` "Shared Response
//! Envelope"). Adoption is intentionally incremental: this helper wraps a
//! route's result DTO in the envelope without requiring every handler to be
//! rewritten in one pass. Started on `/v1/query` and `/v1/retrieve`
//! (U2-18) — remaining routes still return raw DTOs pending a broader pass.

use axon_api::source::{MetadataMap, SuccessEnvelope, TraceContext};

use super::json::Json;

/// Matches `docs/pipeline-unification/surfaces/rest-contract.md`'s
/// "Last Modified" date — the contract's own versioning scheme (a date
/// string, not semver) until a dedicated version field is introduced there.
const CONTRACT_VERSION: &str = "2026-06-30";

/// Wrap `data` in the contract [`SuccessEnvelope`] with a fresh `req_`-
/// prefixed request id and `trace_`-prefixed trace id. `warnings`,
/// `pagination`, `job`, and `artifacts` are left at their empty/`None`
/// defaults — callers that carry real values for those fields should build
/// [`SuccessEnvelope`] directly instead of using this helper.
pub(crate) fn ok<T>(data: T) -> Json<SuccessEnvelope<T>> {
    Json(SuccessEnvelope {
        ok: true,
        contract_version: CONTRACT_VERSION.to_string(),
        data,
        warnings: Vec::new(),
        request_id: format!("req_{}", uuid::Uuid::new_v4()),
        trace: TraceContext {
            trace_id: format!("trace_{}", uuid::Uuid::new_v4()),
            span_id: None,
            parent_span_id: None,
            sampled: false,
            attributes: MetadataMap::default(),
        },
        pagination: None,
        job: None,
        artifacts: Vec::new(),
    })
}

#[cfg(test)]
#[path = "envelope_tests.rs"]
mod tests;
