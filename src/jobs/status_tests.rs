use super::*;

#[test]
fn as_str_returns_expected_values() {
    assert_eq!(JobStatus::Pending.as_str(), "pending");
    assert_eq!(JobStatus::Running.as_str(), "running");
    assert_eq!(JobStatus::Completed.as_str(), "completed");
    assert_eq!(JobStatus::Failed.as_str(), "failed");
    assert_eq!(JobStatus::Canceled.as_str(), "canceled");
}

#[test]
fn display_matches_as_str() {
    for status in [
        JobStatus::Pending,
        JobStatus::Running,
        JobStatus::Completed,
        JobStatus::Failed,
        JobStatus::Canceled,
    ] {
        assert_eq!(format!("{status}"), status.as_str());
    }
}

#[test]
fn all_variants_have_unique_string_representations() {
    let variants = [
        JobStatus::Pending,
        JobStatus::Running,
        JobStatus::Completed,
        JobStatus::Failed,
        JobStatus::Canceled,
    ];
    let strings: std::collections::HashSet<_> = variants.iter().map(|s| s.as_str()).collect();
    assert_eq!(strings.len(), variants.len());
}
