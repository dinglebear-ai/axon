use super::*;

#[test]
fn round_trips_known_dates() {
    let samples = [
        (1970, 1, 1),
        (2024, 1, 15),
        (2023, 11, 14),
        (2000, 2, 29),
        (1999, 12, 31),
        (1900, 3, 1),
        (2400, 2, 29),
    ];
    for (y, m, d) in samples {
        let days = days_from_civil(y, m, d);
        let (ry, rm, rd) = civil_from_days(days);
        assert_eq!(
            (ry, rm, rd),
            (y, m as u32, d as u32),
            "round trip for {y}-{m}-{d}"
        );
    }
}

#[test]
fn civil_from_days_matches_known_epoch_value() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
}

#[test]
fn days_from_civil_matches_known_epoch_value() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
}
