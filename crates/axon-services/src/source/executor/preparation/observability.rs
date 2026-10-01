//! Parser outcomes and actual chunk methods are separate observation dimensions.
use std::collections::BTreeMap;

use axon_document::{PreparationObservation, PrepareSourceDocumentResult};

#[derive(Debug, Default, PartialEq)]
pub(super) struct PreparationSummary {
    pub(super) documents: u64,
    pub(super) skipped: u64,
    pub(super) ast_clean: u64,
    pub(super) ast_recovered: u64,
    pub(super) ast_partial: u64,
    pub(super) ast_zero_symbol: u64,
    pub(super) heuristic_fallback: u64,
    pub(super) unsupported_grammar: u64,
    pub(super) parse_failure: u64,
    pub(super) not_observed: u64,
    pub(super) chunk_methods: BTreeMap<String, u64>,
}

impl PreparationSummary {
    pub(super) fn observe(
        &mut self,
        result: &PrepareSourceDocumentResult,
        observation: &PreparationObservation,
    ) {
        self.documents += 1;
        match observation.outcome_label() {
            "ast_clean" => self.ast_clean += 1,
            "ast_recovered" => self.ast_recovered += 1,
            "ast_partial" => self.ast_partial += 1,
            "ast_zero_symbol" => self.ast_zero_symbol += 1,
            "unsupported_grammar" => self.unsupported_grammar += 1,
            "parse_failure" => self.parse_failure += 1,
            _ => self.not_observed += 1,
        }
        self.heuristic_fallback += u64::from(observation.heuristic_fallback());
        match result {
            PrepareSourceDocumentResult::Skipped(_) => self.skipped += 1,
            PrepareSourceDocumentResult::Prepared(document) => {
                for chunk in &document.chunks {
                    let method = chunk
                        .metadata
                        .get("actual_chunking_method")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(&document.chunking_method);
                    *self
                        .chunk_methods
                        .entry(method_label(method).to_string())
                        .or_default() += 1;
                }
            }
        }
    }

    pub(super) fn message(&self) -> String {
        let attempts = self.ast_clean
            + self.ast_recovered
            + self.ast_partial
            + self.ast_zero_symbol
            + self.parse_failure;
        format!(
            "prepared source batch: documents={} skipped={} AST clean={} recovered={} partial_zero_symbol={} zero_symbol={} supported_attempts={} parse_failure={} unsupported_grammar={} heuristic_fallback={} not_observed={} chunk_methods={}",
            self.documents,
            self.skipped,
            self.ast_clean,
            self.ast_recovered,
            self.ast_partial,
            self.ast_zero_symbol,
            attempts,
            self.parse_failure,
            self.unsupported_grammar,
            self.heuristic_fallback,
            self.not_observed,
            serde_json::to_string(&self.chunk_methods).expect("string counts serialize")
        )
    }
}

// Metadata can originate outside this preparer. Only emit established method
// names, never arbitrary caller strings, paths, or source content in logs.
fn method_label(method: &str) -> &str {
    match method {
        "tree_sitter"
        | "regex_fallback"
        | "code_blocks"
        | "line_window"
        | "structured_manifest"
        | "atomic_manifest"
        | "heading_sections"
        | "plain_text_windows"
        | "dom_to_markdown"
        | "paragraph_windows"
        | "timestamp_turns"
        | "line_segments"
        | "structured_records"
        | "schema_records"
        | "command_records"
        | "turn_segments"
        | "atomic_metadata"
        | "atomic_text"
        | "atomic_fallback"
        | "heuristic_symbol"
        | "atomic_code"
        | "source_context"
        | "source_adjacent_packing" => method,
        _ => "unrecognized",
    }
}

/// Preparation may consume caller-supplied facts or skip before parsing.
/// Keep its event separate from the parser invocation event and aggregate once.
pub(super) fn trace_document(
    result: &PrepareSourceDocumentResult,
    observation: &PreparationObservation,
) {
    let (document_id, disposition, chunks) = match result {
        PrepareSourceDocumentResult::Prepared(document) => {
            (&document.document_id, "prepared", document.chunks.len())
        }
        PrepareSourceDocumentResult::Skipped(document) => {
            (&document.document_id, document.reason.as_str(), 0)
        }
    };
    tracing::info!(target: "axon_services::source::preparation", document_id = %document_id.0,
        disposition, chunks, code_parse_outcome = observation.outcome_label(),
        code_ast_status = observation.ast_status().unwrap_or("not_observed"),
        code_grammar = observation.grammar(), code_symbols = observation.symbol_count(),
        heuristic_fallback = observation.heuristic_fallback(), "document preparation outcome");
}

#[cfg(test)]
#[path = "observability_tests.rs"]
mod tests;
