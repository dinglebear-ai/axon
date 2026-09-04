use super::*;

#[cfg(desktop)]
#[test]
fn center_position_accounts_for_monitor_origin_and_scale() {
    assert_eq!(
        center_position((100, 50), (1600, 1000), 2.0, (400.0, 300.0)),
        (500, 250)
    );
}
