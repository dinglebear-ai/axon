use super::*;

#[test]
fn redaction_metadata_round_trips_all_contract_fields() {
    let report = RedactionMetadata {
        redaction_status: RedactionStatus::Redacted,
        redaction_version: "2026-07-16".to_string(),
        visibility: Visibility::Public,
        redacted_field_count: 2,
        dropped_field_count: 1,
        detector_count: 2,
        detector_names: vec!["bearer_token".to_string(), "secret_field_name".to_string()],
    };
    let write = RedactedPublicWrite {
        payload: serde_json::json!({"message": "[REDACTED]"}),
        redaction: report,
    };

    let value = serde_json::to_value(&write).expect("serialize public write");
    let round_trip: RedactedPublicWrite<serde_json::Value> =
        serde_json::from_value(value).expect("deserialize public write");

    assert_eq!(round_trip, write);
    assert_eq!(round_trip.redaction.detector_count, 2);
}

#[test]
fn redaction_status_has_only_the_three_contracted_values() {
    for (status, expected) in [
        (RedactionStatus::Clean, "clean"),
        (RedactionStatus::Redacted, "redacted"),
        (RedactionStatus::Failed, "failed"),
    ] {
        assert_eq!(status.as_str(), expected);
        assert_eq!(serde_json::to_value(status).unwrap(), expected);
    }
}
