use super::*;

#[test]
fn idle_exit_flag_overrides_config_default() {
    assert_eq!(resolve_idle_exit_secs(Some(10), 300), 10);
}

#[test]
fn idle_exit_falls_back_to_config_default() {
    assert_eq!(resolve_idle_exit_secs(None, 300), 300);
}

#[test]
fn idle_exit_zero_is_preserved_as_run_forever() {
    assert_eq!(resolve_idle_exit_secs(Some(0), 300), 0);
}

#[test]
fn idle_exit_is_clamped_to_one_day() {
    assert_eq!(resolve_idle_exit_secs(Some(u64::MAX), 300), 86_400);
    assert_eq!(resolve_idle_exit_secs(None, 999_999_999), 86_400);
}
