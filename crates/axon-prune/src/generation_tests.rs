use super::*;
use axon_api::source::ids::SourceGenerationId;

fn g(s: &str) -> SourceGenerationId {
    SourceGenerationId::new(s)
}

#[test]
fn prunable_excludes_current() {
    let all = vec![g("gen-1"), g("gen-2"), g("gen-3")];
    let out = prunable_generations(&all, &g("gen-3"));
    assert_eq!(out, vec![g("gen-1"), g("gen-2")]);
}

#[test]
fn prunable_never_returns_current_even_if_duplicated() {
    let all = vec![g("gen-cur"), g("gen-1"), g("gen-cur")];
    let out = prunable_generations(&all, &g("gen-cur"));
    assert_eq!(out, vec![g("gen-1")]);
}

#[test]
fn retention_keeps_newest_and_fences_current() {
    // newest-first
    let all = vec![g("gen-5"), g("gen-4"), g("gen-3"), g("gen-2"), g("gen-1")];
    // keep 2 newest (gen-5, gen-4); current is gen-3.
    let out = prune_beyond_retention(&all, &g("gen-3"), 2);
    // gen-3 fenced out, leaving gen-2, gen-1.
    assert_eq!(out, vec![g("gen-2"), g("gen-1")]);
}

#[test]
fn retention_keep_all_prunes_nothing() {
    let all = vec![g("gen-2"), g("gen-1")];
    let out = prune_beyond_retention(&all, &g("gen-2"), 5);
    assert!(out.is_empty());
}
