use axon_api::source::*;

use crate::parser::{ParseInput, ParseResult, ParserCapability, SourceParser, stage_header};
use crate::registry::ParserRegistry;

#[derive(Debug, Clone)]
pub struct FakeParser {
    capability: ParserCapability,
    facts: Vec<SourceParseFacts>,
    graph_candidates: Vec<GraphCandidate>,
    warnings: Vec<SourceWarning>,
    errors: Vec<SourceError>,
}

impl FakeParser {
    pub fn new(capability: ParserCapability) -> Self {
        Self {
            capability,
            facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn with_content_kind(mut self, kind: ContentKind) -> Self {
        self.capability.content_kinds.push(kind);
        self
    }

    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.capability.mime_types.push(mime_type.into());
        self
    }

    pub fn with_file_extension(mut self, extension: impl Into<String>) -> Self {
        self.capability.file_extensions.push(extension.into());
        self
    }

    pub fn with_sniff_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.capability.sniff_prefixes.push(prefix.into());
        self
    }

    pub fn with_fact(mut self, fact: SourceParseFacts) -> Self {
        self.facts.push(fact);
        self
    }

    pub fn with_graph_candidate(mut self, candidate: GraphCandidate) -> Self {
        self.graph_candidates.push(candidate);
        self
    }
}

impl SourceParser for FakeParser {
    fn capability(&self) -> &ParserCapability {
        &self.capability
    }

    fn parse(&self, input: &ParseInput) -> ParseResult {
        let status = if self.errors.is_empty() && self.warnings.is_empty() {
            LifecycleStatus::Completed
        } else {
            LifecycleStatus::CompletedDegraded
        };
        ParseResult {
            header: stage_header(
                input,
                status,
                self.warnings.clone(),
                self.errors.first().cloned(),
            ),
            document_id: input.document.document_id.clone(),
            facts: self.facts.clone(),
            graph_candidates: self.graph_candidates.clone(),
            parser_id: self.capability.parser_id.clone(),
            parser_version: self.capability.parser_version.clone(),
            warnings: self.warnings.clone(),
            errors: self.errors.clone(),
        }
    }
}

#[derive(Clone, Default)]
pub struct FakeParserRegistry {
    registry: ParserRegistry,
}

impl FakeParserRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_parser(mut self, parser: FakeParser) -> Self {
        self.registry = self.registry.with_parser(parser);
        self
    }

    pub fn parse(&self, input: &ParseInput) -> ParseResult {
        self.registry.parse(input)
    }
}
