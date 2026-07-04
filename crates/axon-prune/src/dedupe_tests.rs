use super::*;
use axon_api::source::ids::VectorPointId;

fn cand(id: &str, key: &str, score: f32) -> DedupeCandidate {
    DedupeCandidate {
        point_id: VectorPointId::new(id),
        dup_key: key.to_string(),
        score,
    }
}

#[test]
fn keeps_best_scoring_point_per_group() {
    let candidates = vec![
        cand("p1", "k1", 0.9),
        cand("p2", "k1", 0.5),
        cand("p3", "k1", 0.7),
    ];
    let plan = plan_dedupe(&candidates);
    assert_eq!(plan.kept, vec![VectorPointId::new("p1")]);
    assert_eq!(
        plan.to_delete,
        vec![VectorPointId::new("p2"), VectorPointId::new("p3")]
    );
    assert_eq!(plan.delete_count(), 2);
}

#[test]
fn singletons_are_kept_and_delete_nothing() {
    let candidates = vec![cand("p1", "k1", 0.5), cand("p2", "k2", 0.5)];
    let plan = plan_dedupe(&candidates);
    assert_eq!(
        plan.kept,
        vec![VectorPointId::new("p1"), VectorPointId::new("p2")]
    );
    assert!(plan.to_delete.is_empty());
}

#[test]
fn score_ties_break_by_point_id() {
    // Both 0.8; p_a sorts before p_b, so p_a is kept.
    let candidates = vec![cand("p_b", "k1", 0.8), cand("p_a", "k1", 0.8)];
    let plan = plan_dedupe(&candidates);
    assert_eq!(plan.kept, vec![VectorPointId::new("p_a")]);
    assert_eq!(plan.to_delete, vec![VectorPointId::new("p_b")]);
}

#[test]
fn empty_input_is_empty_plan() {
    let plan = plan_dedupe(&[]);
    assert!(plan.kept.is_empty());
    assert!(plan.to_delete.is_empty());
}

#[test]
fn every_group_keeps_exactly_one() {
    let candidates = vec![
        cand("a1", "A", 0.1),
        cand("a2", "A", 0.2),
        cand("b1", "B", 0.9),
        cand("b2", "B", 0.3),
        cand("c1", "C", 0.5),
    ];
    let plan = plan_dedupe(&candidates);
    // 3 groups -> 3 kept, 2 deleted.
    assert_eq!(plan.kept.len(), 3);
    assert_eq!(plan.to_delete.len(), 2);
}
