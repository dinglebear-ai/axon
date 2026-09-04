use super::*;

#[test]
fn scheduler_flush_policy_covers_full_group_final_and_close_without_a_timer() {
    let pool = 512;
    assert!(!should_flush(pool, 1, pool, false, false));
    assert!(should_flush(pool * 3, 2, pool, false, false));
    assert!(should_flush(3, 3, pool, false, false));
    assert!(should_flush(1, 1, pool, true, false));
    assert!(should_flush(1, 1, pool, false, true));
}
