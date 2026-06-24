#[test]
fn doctor_json_includes_mode_capabilities_and_remedies() {
    let report = crate::health::doctor::DoctorReport::sample_for_tests();
    let json = serde_json::to_value(report).expect("serialize doctor report");

    assert!(json["mode"]["local_runtime"].is_string());
    assert!(json["capabilities"].is_array());
    assert!(json["recommendations"].is_array());
    assert!(json["services"]["qdrant"]["effective_url"].is_string());
}
