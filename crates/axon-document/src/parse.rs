//! Bridge from `axon-parse` into document preparation.
//!
//! When a caller does not already supply `SourceParseFacts` (the common case for
//! every acquisition adapter today), the preparer parses the source document
//! itself through the shared `axon-parse` production registry. The extracted
//! facts and graph candidates flow into the `PreparedDocument`, and the selected
//! parser's identity informs parser-driven chunk routing.

use std::sync::OnceLock;

use axon_api::source::{
    GraphCandidate, JobId, SourceDocument, SourceError, SourceParseFacts, SourceWarning, StageId,
};
use axon_parse::builtins::production_registry;
use axon_parse::parser::ParseInput;
use axon_parse::registry::ParserRegistry;
use uuid::Uuid;

use crate::profile::ChunkingProfile;

/// Facts + candidates + diagnostics extracted for one source document, plus the
/// parser that produced them so routing can react to it.
#[derive(Debug, Default, Clone)]
pub(crate) struct DocumentParse {
    pub(crate) parser_id: String,
    pub(crate) parser_version: String,
    pub(crate) parse_facts: Vec<SourceParseFacts>,
    pub(crate) graph_candidates: Vec<GraphCandidate>,
    pub(crate) warnings: Vec<SourceWarning>,
    pub(crate) errors: Vec<SourceError>,
}

impl DocumentParse {
    /// A parser-driven profile override, when the selected parser maps to a
    /// specific chunking profile. Prose parsers (`markdown_headings`) and the
    /// no-parser case return `None`, deferring to the content-kind router.
    pub(crate) fn routed_profile(&self) -> Option<ChunkingProfile> {
        match self.parser_id.as_str() {
            "code_symbols" => Some(ChunkingProfile::CodeSymbol),
            "manifest" => Some(ChunkingProfile::CodeManifest),
            "api_schema" => Some(ChunkingProfile::ApiSchema),
            "session_jsonl" => Some(ChunkingProfile::SessionTurns),
            "tool_output_jsonl" => Some(ChunkingProfile::ToolOutput),
            _ => None,
        }
    }
}

fn registry() -> &'static ParserRegistry {
    static REGISTRY: OnceLock<ParserRegistry> = OnceLock::new();
    REGISTRY.get_or_init(production_registry)
}

/// Parse a source document into facts via the shared `axon-parse` registry.
///
/// Deterministic: no external I/O, no persistence, no LLM. A document that no
/// parser supports degrades cleanly to an empty `DocumentParse` (routing then
/// falls back to the content-kind router). This is the single call site that
/// activates `axon-parse` on the acquisition path.
pub(crate) fn parse_document(document: &SourceDocument) -> DocumentParse {
    let input = ParseInput {
        // Preparation is decoupled from a specific job/stage; stamp deterministic
        // ids derived from the document identity so repeated preparation is
        // stable and candidate keys do not churn.
        job_id: JobId::new(deterministic_uuid("job", &document.document_id.0)),
        stage_id: StageId::new(deterministic_uuid("stage", &document.document_id.0)),
        document: document.clone(),
        requested_parser: None,
    };

    let result = registry().parse(&input);

    tracing::info!(
        target: "axon_document::parse",
        document_id = %document.document_id.0,
        canonical_uri = %document.canonical_uri,
        parser_id = %result.parser_id,
        parser_version = %result.parser_version,
        facts = result.facts.len(),
        graph_candidates = result.graph_candidates.len(),
        "axon-parse produced parse facts for document preparation"
    );

    DocumentParse {
        parser_id: result.parser_id,
        parser_version: result.parser_version,
        parse_facts: result.facts,
        graph_candidates: result.graph_candidates,
        warnings: result.warnings,
        errors: result.errors,
    }
}

fn deterministic_uuid(namespace: &str, seed: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("axon-document::{namespace}::{seed}").as_bytes(),
    )
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
