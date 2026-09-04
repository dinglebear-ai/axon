use super::*;
use std::time::Duration;

#[tokio::test]
async fn producer_failure_cancels_consumer_and_preserves_primary_error() {
    let cancel = CancellationToken::new();
    let producer = async { anyhow::bail!("producer failed first") };
    let consumer = async {
        cancel.cancelled().await;
        anyhow::bail!("consumer observed cancellation")
    };

    let error = tokio::time::timeout(
        Duration::from_secs(1),
        join_cancel_on_error(producer, consumer, &cancel),
    )
    .await
    .expect("counterpart should terminate after cancellation")
    .expect_err("producer failure should fail the scheduler");

    let message = format!("{error:#}");
    assert!(message.starts_with("producer failed first"));
    assert!(message.contains("consumer observed cancellation"));
}

#[tokio::test]
async fn consumer_failure_cancels_producer_and_preserves_primary_error() {
    let cancel = CancellationToken::new();
    let producer = async {
        cancel.cancelled().await;
        anyhow::bail!("producer observed cancellation")
    };
    let consumer = async { anyhow::bail!("consumer failed first") };

    let error = tokio::time::timeout(
        Duration::from_secs(1),
        join_cancel_on_error(producer, consumer, &cancel),
    )
    .await
    .expect("counterpart should terminate after cancellation")
    .expect_err("consumer failure should fail the scheduler");

    let message = format!("{error:#}");
    assert!(message.starts_with("consumer failed first"));
    assert!(message.contains("producer observed cancellation"));
}

#[tokio::test]
async fn producer_failure_bounds_non_cooperative_consumer_settlement() {
    let cancel = CancellationToken::new();
    let error = join_cancel_on_error(
        async { anyhow::bail!("producer failed first") },
        std::future::pending::<anyhow::Result<()>>(),
        &cancel,
    )
    .await
    .expect_err("non-cooperative counterpart must be bounded");
    let message = format!("{error:#}");
    assert!(message.starts_with("producer failed first"));
    assert!(message.contains("consumer cancellation did not settle"));
}

#[tokio::test]
async fn consumer_failure_bounds_non_cooperative_producer_settlement() {
    let cancel = CancellationToken::new();
    let error = join_cancel_on_error(
        std::future::pending::<anyhow::Result<()>>(),
        async { anyhow::bail!("consumer failed first") },
        &cancel,
    )
    .await
    .expect_err("non-cooperative counterpart must be bounded");
    let message = format!("{error:#}");
    assert!(message.starts_with("consumer failed first"));
    assert!(message.contains("producer cancellation did not settle"));
}
