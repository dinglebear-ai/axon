use super::*;

#[test]
fn scheduled_pipeline_groups_stable_context_and_mutable_state() {
    assert!(std::mem::size_of::<ScheduledGenerationContext<'static, 'static>>() > 0);
    assert!(std::mem::size_of::<ScheduledGenerationState<'static>>() > 0);
}
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
