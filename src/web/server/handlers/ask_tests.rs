use super::*;

#[test]
fn apply_ask_overrides_clamps_request_tuning() {
    let mut cfg = Config::default();
    let req = AskRequestBody {
        query: "test".to_string(),
        ask_chunk_limit: Some(1),
        ask_full_docs: Some(999),
        ask_max_context_chars: Some(1),
        ask_hybrid_candidates: Some(999),
        ask_doc_chunk_limit: Some(1),
        ask_doc_fetch_concurrency: Some(0),
        ask_backfill_chunks: Some(999),
        ask_candidate_limit: Some(1),
        ask_min_citations_nontrivial: Some(99),
        ask_authoritative_boost: Some(10.0),
        ..AskRequestBody::default()
    };

    apply_ask_overrides(&mut cfg, &req);

    assert_eq!(cfg.ask_chunk_limit, 3);
    assert_eq!(cfg.ask_full_docs, 20);
    assert!(cfg.ask_full_docs_explicit);
    assert_eq!(cfg.ask_max_context_chars, 20_000);
    assert_eq!(cfg.ask_hybrid_candidates, 500);
    assert_eq!(cfg.ask_doc_chunk_limit, 8);
    assert_eq!(cfg.ask_doc_fetch_concurrency, 1);
    assert_eq!(cfg.ask_backfill_chunks, 20);
    assert_eq!(cfg.ask_candidate_limit, 8);
    assert_eq!(cfg.ask_min_citations_nontrivial, 5);
    assert!((cfg.ask_authoritative_boost - 0.5).abs() < f64::EPSILON);
}

#[test]
fn apply_ask_overrides_marks_full_docs_as_explicit() {
    let mut cfg = Config::default();
    let req = AskRequestBody {
        query: "test".to_string(),
        ask_full_docs: Some(2),
        ..AskRequestBody::default()
    };

    apply_ask_overrides(&mut cfg, &req);

    assert_eq!(cfg.ask_full_docs, 2);
    assert!(cfg.ask_full_docs_explicit);
}
