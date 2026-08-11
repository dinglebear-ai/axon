use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::*;

#[tokio::test]
async fn bounded_blocking_map_runs_concurrently_and_preserves_input_order() {
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let observed_active = Arc::clone(&active);
    let observed_maximum = Arc::clone(&maximum);

    let output = bounded_blocking_map_in_order(
        &(0_usize..8).collect::<Vec<_>>(),
        3,
        32,
        |_| 1,
        move |item| {
            let now = observed_active.fetch_add(1, Ordering::SeqCst) + 1;
            observed_maximum.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(20));
            observed_active.fetch_sub(1, Ordering::SeqCst);
            Ok(item * 2)
        },
    )
    .await
    .expect("bounded blocking map");

    assert_eq!(output, vec![0, 2, 4, 6, 8, 10, 12, 14]);
    assert!((2..=3).contains(&maximum.load(Ordering::SeqCst)));
}

#[tokio::test]
async fn bounded_blocking_map_allows_one_oversized_item_to_make_progress() {
    let output = tokio::time::timeout(
        Duration::from_secs(1),
        bounded_blocking_map_in_order(&[8_usize], 2, 4, |item| *item, Ok),
    )
    .await
    .expect("oversized item must not deadlock")
    .expect("bounded blocking map");

    assert_eq!(output, vec![8]);
}
