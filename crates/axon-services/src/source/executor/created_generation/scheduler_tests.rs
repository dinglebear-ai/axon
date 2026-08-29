use super::*;

#[tokio::test(start_paused = true)]
async fn oldest_deadline_is_not_reset_by_later_arrivals() {
    let first = Instant::now() + DEFAULT_FLUSH_DELAY;
    tokio::time::advance(Duration::from_millis(1)).await;
    let later = Instant::now() + DEFAULT_FLUSH_DELAY;
    assert!(first < later);
    tokio::time::advance(DEFAULT_FLUSH_DELAY - Duration::from_millis(1)).await;
    assert!(Instant::now() >= first);
}

#[test]
fn flush_delay_uses_default_and_clamps_configured_values() {
    assert_eq!(flush_delay_from_value(None), DEFAULT_FLUSH_DELAY);
    assert_eq!(flush_delay_from_value(Some("invalid")), DEFAULT_FLUSH_DELAY);
    assert_eq!(flush_delay_from_value(Some("0")), Duration::ZERO);
    assert_eq!(
        flush_delay_from_value(Some("2750")),
        Duration::from_millis(2_750)
    );
    assert_eq!(
        flush_delay_from_value(Some("999999")),
        Duration::from_millis(5_000)
    );
}
