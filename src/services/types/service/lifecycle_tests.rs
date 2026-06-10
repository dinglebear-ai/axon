use super::*;

#[test]
fn job_list_truncation_uses_saturating_add() {
    let result = JobListResult::<()>::new(vec![], i64::MAX, 1, i64::MAX);

    assert!(!result.is_truncated());
}

#[test]
fn job_list_result_clamps_negative_pagination_metadata() {
    let result = JobListResult::<()>::new(vec![], -1, -2, -3);

    assert_eq!(result.total, 0);
    assert_eq!(result.limit, 0);
    assert_eq!(result.offset, 0);
    assert!(!result.is_truncated());
}
