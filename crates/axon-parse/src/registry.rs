use std::sync::Arc;

use axon_api::source::*;

use crate::parser::{ParseInput, ParseResult, SourceParser, stage_header};

#[derive(Clone, Default)]
pub struct ParserRegistry {
    parsers: Vec<Arc<dyn SourceParser>>,
}

impl ParserRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_parser(mut self, parser: impl SourceParser + 'static) -> Self {
        self.parsers.push(Arc::new(parser));
        self.parsers
            .sort_by_key(|parser| parser.capability().priority);
        self
    }

    pub fn select(&self, input: &ParseInput) -> Option<Arc<dyn SourceParser>> {
        self.select_explicit(input)
            .or_else(|| self.select_by(|parser| parser.capability().matches_content_kind(input)))
            .or_else(|| self.select_by(|parser| parser.capability().matches_mime_type(input)))
            .or_else(|| self.select_by(|parser| parser.capability().matches_path(input)))
            .or_else(|| self.select_by(|parser| parser.capability().matches_sniffing(input)))
    }

    pub fn parse(&self, input: &ParseInput) -> ParseResult {
        if let Some(parser) = self.select(input) {
            return parser.parse(input);
        }
        unsupported_result(input)
    }

    fn select_explicit(&self, input: &ParseInput) -> Option<Arc<dyn SourceParser>> {
        let requested = input.requested_parser.as_ref().or_else(|| {
            input
                .document
                .parser_hints
                .first()
                .map(|hint| &hint.parser_id)
        })?;
        self.select_by(|parser| parser.capability().parser_id == *requested)
    }

    fn select_by(
        &self,
        predicate: impl Fn(&Arc<dyn SourceParser>) -> bool,
    ) -> Option<Arc<dyn SourceParser>> {
        self.parsers
            .iter()
            .find(|parser| predicate(parser))
            .cloned()
    }
}

fn unsupported_result(input: &ParseInput) -> ParseResult {
    let warning = SourceWarning {
        code: "parse.unsupported".to_string(),
        severity: Severity::Warning,
        message: format!(
            "no parser registered for content kind {:?}",
            input.document.content_kind
        ),
        source_item_key: Some(input.document.source_item_key.clone()),
        retryable: false,
    };
    ParseResult {
        header: stage_header(
            input,
            LifecycleStatus::CompletedDegraded,
            vec![warning.clone()],
            None,
        ),
        document_id: input.document.document_id.clone(),
        facts: Vec::new(),
        graph_candidates: Vec::new(),
        parser_id: "none".to_string(),
        parser_version: "0".to_string(),
        warnings: vec![warning],
        errors: Vec::new(),
    }
}
