use super::*;

#[test]
fn projection_limits_effective_limit_never_raises_caller_or_fixed_limit() {
    assert_eq!(effective_limit(Some(2), Some(1), 100), Some(1));
    assert_eq!(effective_limit(Some(2), None, 100), Some(2));
}

#[test]
fn projection_limits_oversized_unicode_input_is_measured_in_bytes() {
    assert!(validate_input_bytes("🦀🦀", 7).is_err());
    assert!(validate_input_bytes("🦀🦀", 8).is_ok());
}

#[test]
fn projection_limits_clamp_every_owned_source_unit_downward() {
    let policy = ProjectionBatchConfig {
        max_pages: 10,
        max_manifest_items: 20,
        max_fetched_bytes_per_item: 30,
        max_prepared_bytes: 40,
        max_chunks: 50,
        ..ProjectionBatchConfig::default()
    };
    let mut limits = SourceLimits {
        max_pages: Some(100),
        max_items: Some(2),
        max_bytes_per_item: None,
        max_total_bytes: Some(400),
        max_chunks: Some(5),
        ..SourceLimits::default()
    };
    apply_source_limits(&mut limits, None, &policy);
    assert_eq!(limits.max_pages, Some(10));
    assert_eq!(limits.max_items, Some(2));
    assert_eq!(limits.max_bytes_per_item, Some(30));
    assert_eq!(limits.max_total_bytes, Some(40));
    assert_eq!(limits.max_chunks, Some(5));
}
