//! Source-range validation for parser output.
//!
//! parsing-contract.md requires parser-produced facts/candidates to carry
//! valid provenance, and chunking-contract.md treats "source ranges are
//! impossible or unordered" as a document-reject condition downstream. A
//! parser must never publish a bad span, so `axon-parse` sanitizes every
//! `ParseResult` before it leaves the registry: facts and graph-candidate
//! evidence with an impossible/unordered range are dropped and the result is
//! degraded with a warning rather than silently forwarded.

use axon_api::source::{LifecycleStatus, Severity, SourceRange, SourceWarning};

use crate::parser::ParseResult;

pub const INVALID_RANGE_WARNING_CODE: &str = "parse.invalid_source_range";

/// True when every start/end pair present on `range` is ordered (`start <=
/// end`). A range with no bounds set at all, or only one side of a pair set,
/// is treated as valid — there is nothing to violate. `turn_start`/`turn_end`
/// are opaque session-turn identifiers, not orderable positions, so they are
/// not checked here.
pub fn is_valid_range(range: &SourceRange) -> bool {
    ordered(range.line_start, range.line_end)
        && ordered(range.byte_start, range.byte_end)
        && ordered(range.char_start, range.char_end)
        && ordered(range.time_start_ms, range.time_end_ms)
}

fn ordered<T: PartialOrd>(start: Option<T>, end: Option<T>) -> bool {
    match (start, end) {
        (Some(start), Some(end)) => start <= end,
        _ => true,
    }
}

/// Drop facts and graph-candidate evidence with an impossible/unordered
/// source range, downgrading a `Completed` result to `CompletedDegraded` and
/// attaching a warning when anything was dropped. Idempotent and a no-op when
/// every range is valid.
pub fn sanitize_result(mut result: ParseResult) -> ParseResult {
    let facts_before = result.facts.len();
    result
        .facts
        .retain(|fact| fact.range.as_ref().is_none_or(is_valid_range));
    let dropped_facts = facts_before - result.facts.len();

    let candidates_before = result.graph_candidates.len();
    let mut dropped_evidence = 0usize;
    result.graph_candidates.retain_mut(|candidate| {
        let evidence_before = candidate.evidence.len();
        candidate
            .evidence
            .retain(|evidence| evidence.range.as_ref().is_none_or(is_valid_range));
        let removed = evidence_before - candidate.evidence.len();
        dropped_evidence += removed;
        // Only drop the candidate when filtering actually emptied it — a
        // candidate that legitimately carries no evidence at all (evidence
        // is optional supporting data) must not be treated as invalid.
        removed == 0 || !candidate.evidence.is_empty()
    });
    let dropped_candidates = candidates_before - result.graph_candidates.len();

    if dropped_facts == 0 && dropped_evidence == 0 && dropped_candidates == 0 {
        return result;
    }

    let warning = SourceWarning {
        code: INVALID_RANGE_WARNING_CODE.to_string(),
        severity: Severity::Warning,
        message: format!(
            "dropped {dropped_facts} fact(s) and {dropped_candidates} graph candidate(s) with \
             impossible/unordered source ranges ({dropped_evidence} evidence entrie(s) removed)"
        ),
        source_item_key: None,
        retryable: false,
    };
    if result.header.status == LifecycleStatus::Completed {
        result.header.status = LifecycleStatus::CompletedDegraded;
    }
    result.header.warnings.push(warning.clone());
    result.warnings.push(warning);
    result
}

#[cfg(test)]
#[path = "validate_tests.rs"]
mod tests;
