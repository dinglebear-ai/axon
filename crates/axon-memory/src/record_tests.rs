use super::*;

#[test]
fn parses_epoch_at_unix_zero() {
    assert_eq!(parse_epoch_secs("1970-01-01T00:00:00Z"), Some(0));
}

#[test]
fn parses_known_timestamp() {
    // 2026-01-01T00:00:00Z = 1767225600.
    assert_eq!(
        parse_epoch_secs("2026-01-01T00:00:00Z"),
        Some(1_767_225_600)
    );
}

#[test]
fn parse_and_format_round_trip() {
    for secs in [0, 1_000_000, 1_767_225_600, 1_767_312_045] {
        let formatted = format_rfc3339(secs);
        assert_eq!(parse_epoch_secs(&formatted), Some(secs), "ts={formatted}");
    }
}

#[test]
fn applies_positive_and_negative_offsets() {
    // 01:00:00+01:00 == 00:00:00Z
    let with_offset = parse_epoch_secs("2026-01-01T01:00:00+01:00").unwrap();
    let utc = parse_epoch_secs("2026-01-01T00:00:00Z").unwrap();
    assert_eq!(with_offset, utc);
    // 23:00:00-01:00 == 00:00:00Z next day
    let neg = parse_epoch_secs("2025-12-31T23:00:00-01:00").unwrap();
    assert_eq!(neg, utc);
}

#[test]
fn handles_fractional_seconds() {
    let frac = parse_epoch_secs("2026-01-01T00:00:00.523Z").unwrap();
    assert_eq!(frac, 1_767_225_600);
}

#[test]
fn rejects_malformed_input() {
    assert_eq!(parse_epoch_secs("not-a-date"), None);
    assert_eq!(parse_epoch_secs("2026-13-01T00:00:00Z"), None);
    assert_eq!(parse_epoch_secs(""), None);
}

#[test]
fn age_days_prefers_last_reinforced_over_history() {
    use axon_api::source::{
        MemoryDecayPolicy, MemoryHistoryEvent, MemoryId, MemoryRecord, MemoryScope, MemoryStatus,
        MemoryType, Timestamp,
    };
    let record = MemoryRecord {
        memory_id: MemoryId::new("m"),
        memory_type: MemoryType::Fact,
        status: MemoryStatus::Active,
        body: "b".to_string(),
        confidence: 0.5,
        salience: 0.5,
        scope: MemoryScope {
            kind: "global".to_string(),
            value: "global".to_string(),
        },
        history: vec![MemoryHistoryEvent {
            status: MemoryStatus::Active,
            message: "created".to_string(),
            // 10 days before "now"
            timestamp: Timestamp("2026-01-01T00:00:00Z".to_string()),
        }],
        title: None,
        links: Vec::new(),
        decay: Some(MemoryDecayPolicy {
            profile: "normal".to_string(),
            half_life_days: Some(30),
            // reinforced 1 day before "now"
            last_reinforced_at: Some(Timestamp("2026-01-10T00:00:00Z".to_string())),
            reinforcement_count: 1,
            review_after: None,
            expires_at: None,
            pinned: false,
        }),
        embedding_refs: Vec::new(),
        superseded_by: None,
        contradicts: None,
    };
    // now = 2026-01-11T00:00:00Z
    let now = parse_epoch_secs("2026-01-11T00:00:00Z").unwrap();
    let age = age_days(&record, now);
    // last_reinforced_at is 1 day before now, not the 10-day-old creation.
    assert!((age - 1.0).abs() < 1e-6, "age should be 1 day, got {age}");
}
