use super::*;

#[test]
fn fmt_age_secs_just_now() {
    assert_eq!(fmt_age_secs(0), "just now");
    assert_eq!(fmt_age_secs(59), "just now");
}

#[test]
fn fmt_age_secs_minutes() {
    assert_eq!(fmt_age_secs(60), "1m ago");
    assert_eq!(fmt_age_secs(3_599), "59m ago");
}

#[test]
fn fmt_age_secs_hours_no_minutes() {
    assert_eq!(fmt_age_secs(3_600), "1h ago");
    assert_eq!(fmt_age_secs(7_200), "2h ago");
}

#[test]
fn fmt_age_secs_hours_with_minutes() {
    assert_eq!(fmt_age_secs(3_660), "1h 1m ago");
    assert_eq!(fmt_age_secs(86_399), "23h 59m ago");
}

#[test]
fn fmt_age_secs_days_no_hours() {
    assert_eq!(fmt_age_secs(86_400), "1d ago");
    assert_eq!(fmt_age_secs(172_800), "2d ago");
}

#[test]
fn fmt_age_secs_days_with_hours() {
    assert_eq!(fmt_age_secs(90_000), "1d 1h ago");
    assert_eq!(fmt_age_secs(93_600), "1d 2h ago");
}
