//! Request/result types for document preparation.

use axon_api::source::{
    GraphCandidate, PreparedDocument, SkippedDocument, SourceDocument, SourceError,
    SourceGenerationId, SourceParseFacts, SourceWarning,
};

use crate::profile::ChunkingProfile;

#[derive(Debug, Clone, PartialEq)]
pub struct PrepareSourceDocumentRequest {
    pub document: SourceDocument,
    pub generation: SourceGenerationId,
    pub profile: Option<ChunkingProfile>,
    pub parse_facts: Vec<SourceParseFacts>,
    pub graph_candidates: Vec<GraphCandidate>,
    pub warnings: Vec<SourceWarning>,
    pub errors: Vec<SourceError>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrepareSourceDocumentResult {
    Prepared(Box<PreparedDocument>),
    Skipped(SkippedDocument),
}

/// Parser observation retained even when quality policy skips searchable output.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PreparationObservation {
    pub(crate) code_parse_outcome: Option<serde_json::Value>,
}

impl PreparationObservation {
    /// Reject invalid claims before they reach metadata, counters, or logs.
    pub fn from_value(value: &serde_json::Value) -> Self {
        if !valid_code_outcome(value) {
            return Self::default();
        }
        Self {
            code_parse_outcome: Some(value.clone()),
        }
    }

    /// Explicit AST status from the parser; absence never implies success.
    pub fn ast_status(&self) -> Option<&str> {
        self.code_parse_outcome
            .as_ref()?
            .get("code_ast_status")?
            .as_str()
    }

    pub fn outcome_label(&self) -> &'static str {
        match self.ast_status() {
            Some("parsed") if self.symbol_count() == Some(0) => "ast_zero_symbol",
            Some("parsed") if self.symbol_count().is_some() => "ast_clean",
            Some("partial") if self.symbol_count() == Some(0) => "ast_partial",
            Some("partial") if self.symbol_count().is_some() => "ast_recovered",
            Some("unsupported") => "unsupported_grammar",
            Some("failed") => "parse_failure",
            _ => "not_observed",
        }
    }

    pub fn grammar(&self) -> Option<&str> {
        self.code_parse_outcome
            .as_ref()?
            .get("code_grammar")?
            .as_str()
    }

    pub fn symbol_count(&self) -> Option<u64> {
        self.code_parse_outcome
            .as_ref()?
            .get("code_symbol_count")?
            .as_u64()
    }

    pub fn heuristic_fallback(&self) -> bool {
        self.code_parse_outcome
            .as_ref()
            .and_then(|value| value.get("symbol_extraction_status"))
            .and_then(serde_json::Value::as_str)
            == Some("heuristic_fallback")
    }
}

fn valid_code_outcome(value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "code_ast_status" | "code_grammar" | "code_symbol_count" | "symbol_extraction_status"
        )
    }) {
        return false;
    }
    let Some(status) = value
        .get("code_ast_status")
        .and_then(serde_json::Value::as_str)
    else {
        return false;
    };
    let Some(count) = value
        .get("code_symbol_count")
        .and_then(serde_json::Value::as_u64)
    else {
        return false;
    };
    let extraction = value
        .get("symbol_extraction_status")
        .and_then(serde_json::Value::as_str);
    let grammar = value.get("code_grammar");
    let known_grammar = grammar
        .and_then(serde_json::Value::as_str)
        .is_some_and(|grammar| {
            matches!(
                grammar,
                "rust" | "python" | "javascript" | "typescript" | "tsx" | "bash" | "css" | "elixir"
            )
        });
    let no_grammar = grammar.is_none_or(serde_json::Value::is_null);
    match status {
        "parsed" | "partial" => known_grammar && extraction == Some("ast"),
        "unsupported" | "failed" => {
            no_grammar
                && extraction
                    == Some(if count == 0 {
                        "none"
                    } else {
                        "heuristic_fallback"
                    })
        }
        _ => false,
    }
}
