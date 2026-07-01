use super::*;
use chrono::{TimeZone, Utc};

#[test]
fn cooling_builder_sets_fields() {
    let until = Utc.with_ymd_and_hms(2026, 6, 30, 20, 25, 0).unwrap();
    let cooling = ProviderCooling::new(until)
        .with_provider("tei")
        .with_reason("rate limited");
    assert_eq!(cooling.provider_id.as_deref(), Some("tei"));
    assert_eq!(cooling.cooldown_until, until);
    assert_eq!(cooling.reason.as_deref(), Some("rate limited"));
}

#[test]
fn round_trips_serde() {
    let until = Utc.with_ymd_and_hms(2026, 6, 30, 20, 25, 0).unwrap();
    let cooling = ProviderCooling::new(until).with_provider("tei");
    let value = serde_json::to_value(&cooling).unwrap();
    let back: ProviderCooling = serde_json::from_value(value).unwrap();
    assert_eq!(back, cooling);
}
