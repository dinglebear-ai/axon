use std::sync::Arc;

use axon_api::source::*;

use crate::parser::{ParseInput, ParseResult, ParserCapability, SourceParser, stage_header};

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
        if requested_parser_id(input).is_some() {
            return self.select_explicit(input);
        }
        self.select_best_match(input)
    }

    pub fn parse(&self, input: &ParseInput) -> ParseResult {
        if let Some(requested) = requested_parser_id(input)
            && let Some(parser) =
                self.select_by(|parser| parser.capability().parser_id == *requested)
        {
            return parser.parse(input);
        } else if let Some(requested) = requested_parser_id(input) {
            return requested_parser_unavailable(input, requested);
        }

        if let Some(parser) = self.select(input) {
            return parser.parse(input);
        }
        unsupported_result(input)
    }

    fn select_explicit(&self, input: &ParseInput) -> Option<Arc<dyn SourceParser>> {
        let requested = requested_parser_id(input)?;
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

    fn select_best_match(&self, input: &ParseInput) -> Option<Arc<dyn SourceParser>> {
        let mut best: Option<(u8, u32, Arc<dyn SourceParser>)> = None;

        for parser in &self.parsers {
            let Some(score) = match_score(parser.capability(), input) else {
                continue;
            };
            let priority = parser.capability().priority;
            let should_replace = best.as_ref().is_none_or(|(best_score, best_priority, _)| {
                score > *best_score || (score == *best_score && priority < *best_priority)
            });
            if should_replace {
                best = Some((score, priority, parser.clone()));
            }
        }

        best.map(|(_, _, parser)| parser)
    }
}

fn requested_parser_id(input: &ParseInput) -> Option<&String> {
    input.requested_parser.as_ref().or_else(|| {
        input
            .document
            .parser_hints
            .first()
            .map(|hint| &hint.parser_id)
    })
}

fn match_score(capability: &ParserCapability, input: &ParseInput) -> Option<u8> {
    [
        (50, capability.matches_path(input)),
        (40, capability.matches_mime_type(input)),
        (30, capability.matches_sniffing(input)),
        (10, capability.matches_content_kind(input)),
    ]
    .into_iter()
    .filter_map(|(score, matched)| matched.then_some(score))
    .max()
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

fn requested_parser_unavailable(input: &ParseInput, parser_id: &str) -> ParseResult {
    let warning = SourceWarning {
        code: "parse.requested_parser_unavailable".to_string(),
        severity: Severity::Warning,
        message: format!("requested parser is not registered: {parser_id}"),
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
        parser_id: parser_id.to_string(),
        parser_version: "unavailable".to_string(),
        warnings: vec![warning],
        errors: Vec::new(),
    }
}
