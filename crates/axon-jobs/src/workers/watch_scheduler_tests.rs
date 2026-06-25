use super::*;

#[test]
fn parse_tick_secs_defaults_when_absent_or_invalid() {
    assert_eq!(parse_tick_secs(None), DEFAULT_TICK_SECS);
    assert_eq!(
        parse_tick_secs(Some("not-a-number".to_string())),
        DEFAULT_TICK_SECS
    );
    // Zero is rejected — a 0s ticker would busy-spin.
    assert_eq!(parse_tick_secs(Some("0".to_string())), DEFAULT_TICK_SECS);
}

#[test]
fn parse_tick_secs_accepts_valid_override() {
    assert_eq!(parse_tick_secs(Some("5".to_string())), 5);
}

#[test]
fn parse_lease_secs_defaults_when_absent_or_invalid() {
    assert_eq!(parse_lease_secs(None), DEFAULT_LEASE_SECS);
    assert_eq!(parse_lease_secs(Some("0".to_string())), DEFAULT_LEASE_SECS);
    assert_eq!(
        parse_lease_secs(Some("-10".to_string())),
        DEFAULT_LEASE_SECS
    );
}

#[test]
fn parse_lease_secs_accepts_valid_override() {
    assert_eq!(parse_lease_secs(Some("120".to_string())), 120);
}
