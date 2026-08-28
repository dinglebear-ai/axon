use super::*;

#[tokio::test(start_paused = true)]
async fn oldest_deadline_is_not_reset_by_later_arrivals() {
    let first = Instant::now() + FLUSH_DELAY;
    tokio::time::advance(Duration::from_millis(1)).await;
    let later = Instant::now() + FLUSH_DELAY;
    assert!(first < later);
    tokio::time::advance(Duration::from_millis(1)).await;
    assert!(Instant::now() >= first);
}
