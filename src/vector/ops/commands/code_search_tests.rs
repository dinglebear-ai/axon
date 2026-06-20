use super::*;

#[test]
fn code_search_score_policy_forces_code_intent_without_topical_gate() {
    let policy = code_search_score_policy();
    assert!(policy.apply_code_search_adjustment);
    assert!(policy.force_code_intent);
    assert!(!policy.require_topical_overlap);
    assert_eq!(policy.min_relevance_score, None);
}
