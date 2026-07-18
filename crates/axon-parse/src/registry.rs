use std::sync::Arc;

use axon_api::source::*;

use crate::parser::{ParseInput, ParseResult, ParserCapability, SourceParser, stage_header};
use crate::validate::sanitize_result;

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

    /// Select the single "primary" parser for `input` — the caller-requested
    /// or hint-named parser when it resolves, else the best-scored specific
    /// match (MIME type, path/extension, or content sniffing), or, failing
    /// that, the highest-priority content-kind-only match. This is a
    /// one-parser probe sharing `parse`'s channel order; production chunk
    /// routing consumes the parser identity `parse` returns, so this exists
    /// for tests and tooling that need a selection without running a parse.
    pub fn select(&self, input: &ParseInput) -> Option<Arc<dyn SourceParser>> {
        if let Some(requested) = input.requested_parser.as_deref() {
            return self.select_by_id(requested);
        }
        if let Some(hinted) = self.hinted_parser(input) {
            return Some(hinted);
        }
        self.ranked_matches(input)
            .into_iter()
            .next()
            .map(|(_, parser)| parser)
    }

    /// Parse `input` per parsing-contract.md's selection order:
    ///
    /// 1. an explicit `requested_parser` runs alone — exclusive. A request for
    ///    an unregistered parser degrades without fallback: the caller demanded
    ///    one specific parser and nothing else may answer for it.
    /// 2. a document `ParserHint` naming a registered parser also runs alone —
    ///    exclusive like a request, suppressing the fan-out below. Only the
    ///    document's first hint is consulted; later entries are ignored.
    ///    Hints are advisory metadata stamped by upstream stages, so a hint
    ///    naming an unregistered parser falls back to content-based selection
    ///    below (recording an informational warning, on the unsupported path
    ///    too) instead of degrading every hinted document.
    /// 3. otherwise every parser that specifically identifies the document
    ///    (MIME type, path/extension, or content sniffing) runs and their
    ///    facts/graph candidates/warnings/errors merge into one result, since
    ///    "Multiple parsers may run when they emit different fact families"
    ///    (e.g. `docker-compose.yaml` gets both generic manifest facts and
    ///    Docker-specific facts).
    /// 4. when nothing matches specifically, fall back to a single
    ///    content-kind-only match (the weakest, last-resort signal).
    /// 5. when nothing matches at all, the input is `Skipped`, not `Failed`
    ///    or `CompletedDegraded` — an unsupported item must not fail the job.
    ///
    /// Every result is sanitized before it leaves the registry: facts and
    /// graph-candidate evidence with an impossible/unordered source range are
    /// dropped and the result is degraded (see `validate::sanitize_result`).
    pub fn parse(&self, input: &ParseInput) -> ParseResult {
        if let Some(requested) = input.requested_parser.as_deref() {
            return match self.select_by_id(requested) {
                Some(parser) => sanitize_result(parser.parse(input)),
                None => requested_parser_unavailable(input, requested),
            };
        }

        if let Some(parser) = self.hinted_parser(input) {
            return sanitize_result(parser.parse(input));
        }

        // Reaching this point with a hint present means it named an
        // unregistered parser (a registered hint returned above). Record that
        // on every fallback outcome — including the unsupported path — so the
        // ignored hint stays observable.
        let stale_hint_warning = input
            .document
            .parser_hints
            .first()
            .map(|hint| unregistered_hint_warning(input, &hint.parser_id));

        let matches = self.ranked_matches(input);
        if matches.is_empty() {
            let mut unsupported = unsupported_result(input);
            unsupported.warnings.extend(stale_hint_warning);
            return unsupported;
        }

        let mut merged = matches[0].1.parse(input);
        for (_, parser) in &matches[1..] {
            merge_result(&mut merged, parser.parse(input));
        }
        merged.warnings.extend(stale_hint_warning);
        sanitize_result(merged)
    }

    /// The parser named by the document's first `ParserHint`, when that hint
    /// resolves to a registered parser. Only the first hint is consulted —
    /// later entries are ignored regardless of registration status. `None`
    /// covers both "no hint" and "hint names an unregistered parser"; callers
    /// that must distinguish the two check `input.document.parser_hints`
    /// themselves.
    fn hinted_parser(&self, input: &ParseInput) -> Option<Arc<dyn SourceParser>> {
        let hinted = input.document.parser_hints.first()?;
        self.select_by_id(&hinted.parser_id)
    }

    fn select_by_id(&self, parser_id: &str) -> Option<Arc<dyn SourceParser>> {
        self.select_by(|parser| parser.capability().parser_id == parser_id)
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

    /// All parsers that match `input`, ranked best-first. When one or more
    /// parsers match via a specific signal (MIME type, path/extension, or
    /// content sniffing) the ranking contains only those specific matches —
    /// every one of them is a positive, self-identifying signal and all are
    /// intended to run together per the contract's multi-parser example.
    /// When none match specifically, the ranking falls back to content-kind
    /// matches alone (at most the single highest-priority one, since
    /// content-kind is a broad, last-resort classification rather than a
    /// distinct identification and should not fan out).
    fn ranked_matches(&self, input: &ParseInput) -> Vec<(u8, Arc<dyn SourceParser>)> {
        let mut specific: Vec<(u8, Arc<dyn SourceParser>)> = self
            .parsers
            .iter()
            .filter_map(|parser| {
                specific_score(parser.capability(), input).map(|score| (score, parser.clone()))
            })
            .collect();
        if !specific.is_empty() {
            specific.sort_by(|(score_a, parser_a), (score_b, parser_b)| {
                score_b.cmp(score_a).then(
                    parser_a
                        .capability()
                        .priority
                        .cmp(&parser_b.capability().priority),
                )
            });
            return specific;
        }

        self.parsers
            .iter()
            .filter(|parser| parser.capability().matches_content_kind(input))
            .min_by_key(|parser| parser.capability().priority)
            .map(|parser| vec![(0u8, parser.clone())])
            .unwrap_or_default()
    }
}

/// Score a parser's specific (non-content-kind) identification signals per
/// parsing-contract.md's order: MIME type, then path/extension, then content
/// sniffing. `None` means the parser did not specifically identify the
/// document at all.
fn specific_score(capability: &ParserCapability, input: &ParseInput) -> Option<u8> {
    [
        (40, capability.matches_mime_type(input)),
        (30, capability.matches_path(input)),
        (20, capability.matches_sniffing(input)),
    ]
    .into_iter()
    .filter_map(|(score, matched)| matched.then_some(score))
    .max()
}

/// Merge a secondary parser's output into the primary (best-matched) result.
/// The header/parser identity stay the primary's; facts, graph candidates,
/// warnings, and errors accumulate. The merged status degrades when any
/// secondary parser did not complete cleanly.
fn merge_result(primary: &mut ParseResult, mut secondary: ParseResult) {
    primary.facts.append(&mut secondary.facts);
    primary
        .graph_candidates
        .append(&mut secondary.graph_candidates);
    primary.warnings.append(&mut secondary.warnings);
    primary.errors.append(&mut secondary.errors);
    primary
        .header
        .warnings
        .append(&mut secondary.header.warnings);
    if secondary.header.status != LifecycleStatus::Completed
        && primary.header.status == LifecycleStatus::Completed
    {
        primary.header.status = LifecycleStatus::CompletedDegraded;
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
        header: stage_header(input, LifecycleStatus::Skipped, vec![warning.clone()], None),
        document_id: input.document.document_id.clone(),
        facts: Vec::new(),
        graph_candidates: Vec::new(),
        parser_id: "none".to_string(),
        parser_version: "0".to_string(),
        warnings: vec![warning],
        errors: Vec::new(),
    }
}

/// Warning attached when a document's advisory `ParserHint` named a parser
/// this registry does not know. The parse itself proceeded via content-based
/// selection, so this is informational — unlike an explicit `requested_parser`
/// miss, which degrades the result (see `requested_parser_unavailable`). The
/// code and wording deliberately differ from `parse.requested_parser_unavailable`
/// so alerts keyed on the strict channel never match advisory-hint fallbacks.
fn unregistered_hint_warning(input: &ParseInput, parser_id: &str) -> SourceWarning {
    SourceWarning {
        code: "parse.parser_hint_unregistered".to_string(),
        severity: Severity::Info,
        message: format!(
            "parser hint does not name a registered parser (used content-based selection): {parser_id}"
        ),
        source_item_key: Some(input.document.source_item_key.clone()),
        retryable: false,
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
