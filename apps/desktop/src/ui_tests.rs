use super::*;

#[test]
fn elapsed_label_subsecond_shows_tenths() {
    assert_eq!(format_elapsed(Duration::from_millis(0)), "0.0s");
    assert_eq!(format_elapsed(Duration::from_millis(400)), "0.4s");
    assert_eq!(format_elapsed(Duration::from_millis(999)), "0.9s");
}

#[test]
fn elapsed_label_seconds_no_decimal() {
    assert_eq!(format_elapsed(Duration::from_secs(1)), "1s");
    assert_eq!(format_elapsed(Duration::from_secs(12)), "12s");
    assert_eq!(format_elapsed(Duration::from_secs(59)), "59s");
}

#[test]
fn elapsed_label_minutes_uses_padded_seconds() {
    assert_eq!(format_elapsed(Duration::from_secs(60)), "1m 00s");
    assert_eq!(format_elapsed(Duration::from_secs(63)), "1m 03s");
    assert_eq!(format_elapsed(Duration::from_secs(125)), "2m 05s");
}
