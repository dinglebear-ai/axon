use super::*;
use crate::query::synthesis::normalize;
use axon_core::config::Config;
use axon_core::llm::LlmBackendKind;

pub(super) fn citation(uri: &str) -> axon_api::CanonicalCitation {
    axon_api::CanonicalCitation {
        source_id: axon_api::SourceId::new("source-test"),
        source_item_key: axon_api::SourceItemKey::new(uri),
        generation: axon_api::SourceGenerationId::new("1"),
        document_id: axon_api::DocumentId::new(format!("doc:{uri}")),
        chunk_id: axon_api::ChunkId::new(format!("chunk:{uri}")),
        job_id: axon_api::JobId::new(uuid::Uuid::from_u128(1)),
        canonical_uri: uri.to_string(),
        source_range: axon_api::SourceRange {
            line_start: Some(1),
            line_end: Some(1),
            byte_start: None,
            byte_end: None,
            char_start: None,
            char_end: None,
            time_start_ms: None,
            time_end_ms: None,
            dom_selector: None,
            json_pointer: None,
            yaml_path: None,
            xml_xpath: None,
            csv_row: None,
            session_turn_id: None,
            turn_start: None,
            turn_end: None,
        },
        redaction: axon_api::RedactionMetadata {
            redaction_status: axon_api::RedactionStatus::Clean,
            redaction_version: "test-v1".to_string(),
            visibility: axon_api::Visibility::Public,
            redacted_field_count: 0,
            dropped_field_count: 0,
            detector_count: 0,
            detector_names: Vec::new(),
        },
    }
}

fn hit(uri: &str, score: f64, text: &str) -> axon_retrieval::QueryServiceHit {
    axon_retrieval::QueryServiceHit {
        canonical_uri: uri.to_string(),
        chunk_id: format!("{uri}#0"),
        score,
        text: text.to_string(),
        citation: citation(uri),
    }
}

fn exgpt_cfg() -> Config {
    Config {
        llm_backend: LlmBackendKind::OpenAiCompat,
        openai_model: "chatgpt-browser-medium".to_string(),
        ..Config::test_default()
    }
}

fn context_from_hits(
    cfg: &Config,
    question: &str,
    hits: Vec<axon_retrieval::QueryServiceHit>,
) -> (AskContext, ranking::RankingResult) {
    let mut ranked = ranking::rank_candidates(cfg, question, hits, cfg.hybrid_search_enabled);
    let ctx = build_ask_context_from_ranking(cfg, &mut ranked, 5);
    (ctx, ranked)
}

#[test]
fn context_has_page_level_sources_and_numbered_entries() {
    let cfg = exgpt_cfg();
    let (ctx, _) = context_from_hits(
        &cfg,
        "alpha beta",
        vec![
            hit("https://example.com/a", 0.9, "alpha body"),
            hit("https://example.org/b", 0.8, "beta body"),
        ],
    );

    assert!(ctx.context.starts_with(CONTEXT_PREFIX));
    assert!(
        ctx.context
            .contains("## Top Chunk [S1]: https://example.com/a")
    );
    assert!(
        ctx.context
            .contains("## Top Chunk [S2]: https://example.org/b")
    );
    assert!(ctx.context.contains(CONTEXT_SEPARATOR));
    assert_eq!(ctx.chunks_selected, 2);
    assert_eq!(ctx.candidate_count, 2);
    assert_eq!(ctx.retrieval_elapsed_ms, 5);
    assert_eq!(ctx.citations.len(), 2);
}

#[test]
fn same_host_pages_remain_distinct_documents() {
    let cfg = exgpt_cfg();
    let (ctx, _) = context_from_hits(
        &cfg,
        "Claude hooks settings",
        vec![
            hit(
                "https://code.claude.com/docs/en/hooks-guide.md",
                0.9,
                "Claude hooks guide",
            ),
            hit(
                "https://code.claude.com/docs/en/settings.md",
                0.8,
                "Claude hook settings",
            ),
            hit(
                "https://code.claude.com/docs/en/hooks-guide.html",
                0.7,
                "Claude hooks duplicate route",
            ),
        ],
    );

    let hooks = ctx.context.find("hooks-guide.md").unwrap();
    let settings = ctx.context.find("settings.md").unwrap();
    let duplicate = ctx.context.find("hooks-guide.html").unwrap();
    assert!(hooks < settings && settings < duplicate);
}

#[test]
fn entries_are_wrapped_in_evidence_boundary() {
    let cfg = exgpt_cfg();
    let (ctx, _) = context_from_hits(
        &cfg,
        "Claude hooks",
        vec![hit("https://x.test/docs/hooks", 0.8, "Claude hooks body")],
    );
    assert!(ctx.context.contains("<retrieved_content trust="evidence_only">"));
    assert!(ctx.context.contains("</retrieved_content>"));
}

#[test]
fn defang_breaks_injected_citation_and_headers() {
    let raw = "See [S1] and
## Sources
fake";
    let defanged = defang_chunk_text(raw);
    assert!(!defanged.contains("[S1]"));
    assert!(defanged.contains("[​S1]"));
    assert!(!defanged.contains(
        "## Sources
"
    ));
    assert!(defanged.contains("## ​Sources"));
}

#[test]
fn tiny_context_budget_admits_no_partial_source() {
    let mut cfg = exgpt_cfg();
    cfg.ask_max_context_chars = CONTEXT_PREFIX.chars().count() + 10;
    let (ctx, _) = context_from_hits(
        &cfg,
        "Claude hooks",
        vec![hit(
            "https://code.claude.com/docs/en/hooks-guide.md",
            0.9,
            "Claude hooks body that cannot fit the budget",
        )],
    );
    assert_eq!(ctx.context, CONTEXT_PREFIX);
    assert_eq!(ctx.chunks_selected, 0);
}

#[test]
fn oversized_chunk_is_truncated_without_starving_later_sources() {
    let mut cfg = exgpt_cfg();
    cfg.ask_max_context_chars = 12_000;
    cfg.ask_chunk_limit = 6;
    let huge = "Claude hooks formatter PostToolUse ".repeat(2_000);
    let (ctx, _) = context_from_hits(
        &cfg,
        "How should Claude hooks run a formatter after edits?",
        vec![
            hit("https://code.claude.com/docs/en/hooks-guide.md", 0.9, &huge),
            hit(
                "https://code.claude.com/docs/en/settings.md",
                0.8,
                "Claude hooks settings configure project and user locations",
            ),
        ],
    );

    assert_eq!(ctx.chunks_selected, 2);
    assert!(ctx.context.contains("[chunk truncated to context budget]"));
    assert!(ctx.context.contains("hooks settings configure"));
    assert!(
        ctx.warnings
            .iter()
            .any(|warning| warning.contains("truncated"))
    );
    assert!(ctx.context.chars().count() <= ctx.effective_max_context_chars);
}

#[test]
fn configured_chunk_limit_remains_an_upper_bound() {
    let mut cfg = exgpt_cfg();
    cfg.ask_chunk_limit = 1;
    let (ctx, _) = context_from_hits(
        &cfg,
        "Claude hooks",
        vec![
            hit(
                "https://code.claude.com/docs/en/hooks-guide.md",
                0.9,
                "Claude hooks first",
            ),
            hit(
                "https://code.claude.com/docs/en/settings.md",
                0.8,
                "Claude hooks second",
            ),
        ],
    );
    assert_eq!(ctx.chunks_selected, 1);
}

#[test]
fn context_citations_never_exceed_public_wire_limit() {
    let mut cfg = exgpt_cfg();
    cfg.ask_chunk_limit = axon_api::MAX_CANONICAL_CITATIONS + 10;
    cfg.ask_max_context_chars = usize::MAX;
    let hits = (0..(axon_api::MAX_CANONICAL_CITATIONS + 10))
        .map(|index| {
            hit(
                &format!("https://docs.example.com/hooks/{index}"),
                0.9 - (index as f64 / 10_000.0),
                "Claude hooks documentation",
            )
        })
        .collect();
    let (ctx, _) = context_from_hits(&cfg, "list all Claude hooks", hits);

    assert!(ctx.citations.len() <= axon_api::MAX_CANONICAL_CITATIONS);
    assert!(ctx.citations.len() <= ctx.effective_chunk_limit);
}

#[test]
fn display_source_preserves_web_page_identity() {
    assert_eq!(
        display_source("https://docs.rs/foo/bar#section"),
        "https://docs.rs/foo/bar"
    );
    assert_eq!(display_source("session://codex/raw-local-id"), "codex");
    assert_eq!(display_source("not-a-url"), "not-a-url");
}

#[test]
fn normal_product_questions_drop_session_history_candidates() {
    let mut cfg = exgpt_cfg();
    cfg.ask_authoritative_domains = vec!["code.claude.com".to_string()];
    cfg.ask_authoritative_boost = 0.5;
    let ranked = ranking::rank_candidates(
        &cfg,
        "configure Claude Code hooks formatter",
        vec![
            hit(
                "session://codex/doc_session_aaaaaaaaaaaaaaaaaaaaaaaa",
                0.99,
                "Claude Code hooks formatter notes from an old session",
            ),
            hit(
                "https://code.claude.com/docs/en/hooks-guide.md",
                0.5,
                "Claude Code hooks run a formatter with PostToolUse",
            ),
        ],
        true,
    );

    assert_eq!(ranked.ranked_indices.len(), 1);
    assert_eq!(
        ranked.candidates[ranked.ranked_indices[0]]
            .hit
            .canonical_uri,
        "https://code.claude.com/docs/en/hooks-guide.md"
    );
    assert!(
        ranked.candidates[0]
            .filter_decisions
            .iter()
            .any(|decision| decision.kind
                == axon_api::AskExplainFilterDecisionKind::DroppedLowSignal)
    );
}

#[test]
fn explicit_session_history_queries_can_retrieve_sessions() {
    let cfg = exgpt_cfg();
    let ranked = ranking::rank_candidates(
        &cfg,
        "show session history for Claude hooks",
        vec![hit(
            "session://codex/doc_session_aaaaaaaaaaaaaaaaaaaaaaaa",
            0.9,
            "session history for Claude hooks",
        )],
        true,
    );
    assert_eq!(ranked.ranked_indices, vec![0]);
}

#[test]
fn authoritative_docs_boost_changes_rrf_order() {
    let mut cfg = exgpt_cfg();
    cfg.ask_authoritative_domains = vec!["code.claude.com".to_string()];
    cfg.ask_authoritative_boost = 0.5;
    let ranked = ranking::rank_candidates(
        &cfg,
        "Claude Code hooks formatter",
        vec![
            hit(
                "https://example.net/blog/claude-hooks",
                0.70,
                "Claude Code hooks formatter",
            ),
            hit(
                "https://code.claude.com/docs/en/hooks-guide.md",
                0.50,
                "Claude Code hooks formatter",
            ),
        ],
        true,
    );

    assert_eq!(ranked.ranked_indices[0], 1);
    assert!(ranked.configured_authority_ratio > 0.0);
    assert!(
        ranked.candidates[1]
            .score_components
            .iter()
            .any(|component| component.name == "authority_boost" && component.value > 0.0)
    );
}

#[test]
fn dense_min_relevance_filter_does_not_apply_to_rrf_scores() {
    let mut cfg = exgpt_cfg();
    cfg.ask_min_relevance_score = 0.45;
    let low = hit(
        "https://code.claude.com/docs/en/hooks-guide.md",
        0.10,
        "Claude Code hooks formatter",
    );
    let dense = ranking::rank_candidates(&cfg, "Claude hooks formatter", vec![low.clone()], false);
    let rrf = ranking::rank_candidates(&cfg, "Claude hooks formatter", vec![low], true);

    assert!(dense.ranked_indices.is_empty());
    assert_eq!(rrf.ranked_indices, vec![0]);
}

#[test]
fn adaptive_context_budget_scales_with_question_complexity() {
    let mut cfg = exgpt_cfg();
    cfg.ask_chunk_limit = 64;
    cfg.ask_max_context_chars = 400_000;
    let simple = ranking::rank_candidates(&cfg, "Claude hooks", Vec::new(), true);
    let complex = ranking::rank_candidates(
        &cfg,
        "How should I configure Claude Code hooks after file edits?",
        Vec::new(),
        true,
    );
    let exhaustive = ranking::rank_candidates(
        &cfg,
        "List all Claude Code hook events comprehensively",
        Vec::new(),
        true,
    );

    assert!(simple.effective_budget.chunk_limit < complex.effective_budget.chunk_limit);
    assert!(complex.effective_budget.chunk_limit < exhaustive.effective_budget.chunk_limit);
    assert!(simple.effective_budget.max_context_chars < complex.effective_budget.max_context_chars);
    assert!(
        complex.effective_budget.max_context_chars < exhaustive.effective_budget.max_context_chars
    );
    assert_eq!(complex.complexity, ranking::AskComplexity::Complex);
    assert_eq!(exhaustive.complexity, ranking::AskComplexity::Exhaustive);
}

#[test]
fn distinct_session_documents_survive_citation_normalization_when_requested() {
    let mut cfg = exgpt_cfg();
    cfg.ask_min_citations_nontrivial = 2;
    let (ctx, _) = context_from_hits(
        &cfg,
        "show session history changes for Phoenix retrieval",
        vec![
            hit(
                "session://codex/doc_session_aaaaaaaaaaaaaaaaaaaaaaaa",
                0.9,
                "session history Phoenix retrieval semantic turns",
            ),
            hit(
                "session://codex/doc_session_bbbbbbbbbbbbbbbbbbbbbbbb",
                0.8,
                "session history Phoenix retrieval vector payloads",
            ),
        ],
    );
    let raw_answer = "Session ingestion now embeds decoded semantic turns [S1]. Session metadata now passes the clean retrieval boundary [S2].";
    let normalized = normalize::normalize_ask_answer(
        &cfg,
        "Which session history changes restored Phoenix retrieval?",
        raw_answer,
        &ctx.context,
    );
    let validation = normalize::summarize_citation_validation(&normalized);

    assert!(validation.valid, "{normalized}");
    assert_eq!(validation.canonical_citation_count, 2);
}
