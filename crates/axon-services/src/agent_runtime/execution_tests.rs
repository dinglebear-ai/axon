use super::*;

#[test]
fn labby_status_polling_backs_off_to_one_second() {
    assert_eq!(labby_status_poll_delay(0), Duration::from_millis(100));
    assert_eq!(labby_status_poll_delay(1), Duration::from_millis(250));
    assert_eq!(labby_status_poll_delay(2), Duration::from_millis(500));
    assert_eq!(labby_status_poll_delay(3), Duration::from_secs(1));
    assert_eq!(labby_status_poll_delay(100), Duration::from_secs(1));
}

#[tokio::test]
async fn status_wait_is_fenced_by_the_turn_deadline() {
    let store = AgentTurnStore::memory().expect("store");
    let deadline = now_ms() + 20;
    let result = await_with_turn_deadline(
        &store,
        "unused-test-turn",
        1,
        deadline,
        tokio::time::sleep(Duration::from_millis(100)),
    )
    .await;
    assert!(
        result.is_none(),
        "status wait must not outlive the turn deadline"
    );
}
