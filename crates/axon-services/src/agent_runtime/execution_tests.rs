use super::*;

#[test]
fn labby_status_polling_backs_off_to_one_second() {
    assert_eq!(labby_status_poll_delay(0), Duration::from_millis(100));
    assert_eq!(labby_status_poll_delay(1), Duration::from_millis(250));
    assert_eq!(labby_status_poll_delay(2), Duration::from_millis(500));
    assert_eq!(labby_status_poll_delay(3), Duration::from_secs(1));
    assert_eq!(labby_status_poll_delay(100), Duration::from_secs(1));
}
