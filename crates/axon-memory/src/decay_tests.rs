use super::*;
use axon_api::source::{
    DecayProfile, MemoryDecayPolicy, MemoryHistoryEvent, MemoryId, MemoryRecord, MemoryScope,
    MemoryStatus, MemoryType, Timestamp, Visibility,
};

fn record(memory_type: MemoryType, status: MemoryStatus) -> MemoryRecord {
    MemoryRecord {
        memory_id: MemoryId::new("m1"),
        memory_type,
        status,
        body: "body".to_string(),
        confidence: 0.8,
        salience: 0.6,
        scope: MemoryScope {
            kind: "project".to_string(),
            value: "axon".to_string(),
        },
        history: vec![MemoryHistoryEvent {
            status,
            message: "created".to_string(),
            timestamp: Timestamp("2026-01-01T00:00:00Z".to_string()),
        }],
        visibility: Visibility::Internal,
        title: None,
        links: Vec::new(),
        decay: None,
        embedding_refs: Vec::new(),
        superseded_by: None,
        contradicts: None,
    }
}

#[test]
fn reinforcement_score_is_log_scaled_and_clamped() {
    assert_eq!(reinforcement_score(0), 0.0);
    let one = reinforcement_score(1);
    assert!(one > 0.0 && one < 0.2, "ln(2)/5 ~= 0.138, got {one}");
    // Large counts clamp to 1.0.
    assert_eq!(reinforcement_score(u32::MAX), 1.0);
    // Monotonic increasing.
    assert!(reinforcement_score(10) > reinforcement_score(5));
}

#[test]
fn status_penalty_matches_contract() {
    assert_eq!(status_penalty(MemoryStatus::Active, false), 0.0);
    assert_eq!(status_penalty(MemoryStatus::Forgotten, false), 1.0);
    assert_eq!(status_penalty(MemoryStatus::Superseded, false), 0.5);
    assert_eq!(status_penalty(MemoryStatus::Archived, false), 0.25);
    // Archived is not penalized when explicitly included.
    assert_eq!(status_penalty(MemoryStatus::Archived, true), 0.0);
}

#[test]
fn decay_multiplier_halves_each_half_life() {
    let p = DecayProfile::Normal; // 30-day half-life
    let at_zero = decay_multiplier(p, false, 0.0);
    let at_one_hl = decay_multiplier(p, false, 30.0);
    let at_two_hl = decay_multiplier(p, false, 60.0);
    assert!((at_zero - 1.0).abs() < 1e-6);
    assert!((at_one_hl - 0.5).abs() < 1e-4, "got {at_one_hl}");
    assert!((at_two_hl - 0.25).abs() < 1e-4, "got {at_two_hl}");
}

#[test]
fn decay_multiplier_is_one_for_pinned_and_none_profile() {
    assert_eq!(decay_multiplier(DecayProfile::Normal, true, 1000.0), 1.0);
    assert_eq!(decay_multiplier(DecayProfile::None, false, 1000.0), 1.0);
}

#[test]
fn default_decay_profiles_follow_type_table() {
    assert_eq!(
        MemoryType::Preference.default_decay_profile(),
        DecayProfile::VerySlow
    );
    assert_eq!(
        MemoryType::Working.default_decay_profile(),
        DecayProfile::VeryFast
    );
    assert_eq!(
        MemoryType::Decision.default_decay_profile(),
        DecayProfile::Slow
    );
    assert_eq!(
        MemoryType::Fact.default_decay_profile(),
        DecayProfile::Normal
    );
    assert_eq!(
        MemoryType::Episode.default_decay_profile(),
        DecayProfile::Fast
    );
}

#[test]
fn score_decreases_with_age() {
    let rec = record(MemoryType::Fact, MemoryStatus::Active);
    let fresh = score_record(&rec, 0.0, 1.0, 1.0, false);
    let aged = score_record(&rec, 60.0, 1.0, 1.0, false);
    assert!(fresh > aged, "fresh {fresh} should exceed aged {aged}");
}

#[test]
fn contradicted_status_lowers_score() {
    let active = record(MemoryType::Fact, MemoryStatus::Active);
    let contradicted = record(MemoryType::Fact, MemoryStatus::Contradicted);
    let a = score_record(&active, 0.0, 1.0, 1.0, false);
    let c = score_record(&contradicted, 0.0, 1.0, 1.0, false);
    assert!(
        (a - c - 0.25).abs() < 1e-5,
        "penalty should be 0.25: {a} vs {c}"
    );
}

#[test]
fn pinned_record_ignores_decay() {
    let mut rec = record(MemoryType::Working, MemoryStatus::Active);
    rec.decay = Some(MemoryDecayPolicy {
        profile: "very_fast".to_string(),
        half_life_days: Some(1),
        last_reinforced_at: None,
        reinforcement_count: 0,
        review_after: None,
        expires_at: None,
        pinned: true,
    });
    let old = score_record(&rec, 365.0, 1.0, 1.0, false);
    let fresh = score_record(&rec, 0.0, 1.0, 1.0, false);
    assert!((old - fresh).abs() < 1e-6, "pinned score must not decay");
}

#[test]
fn base_score_weights_sum_to_one() {
    let inputs = ScoreInputs {
        semantic_score: 1.0,
        confidence: 1.0,
        salience: 1.0,
        scope_match: 1.0,
        reinforcement_score: 1.0,
        decay_multiplier: 1.0,
        contradiction_penalty: 0.0,
        status_penalty: 0.0,
    };
    assert!((base_score(&inputs) - 1.0).abs() < 1e-6);
    assert!((memory_score(&inputs) - 1.0).abs() < 1e-6);
}
