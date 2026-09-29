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
    assert!(message.contains("producer failed first"));
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
    assert!(message.contains("consumer failed first"));
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
    assert!(message.contains("producer failed first"));
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
    assert!(message.contains("consumer failed first"));
    assert!(message.contains("producer cancellation did not settle"));
}

#[test]
fn cancellation_counterpart_does_not_shadow_typed_acquisition_failure() {
    let primary = anyhow::anyhow!("generation scheduler canceled before vectorization");
    let typed = axon_api::source::ApiError::new(
        "adapter.read_failed",
        axon_api::source::ErrorStage::Fetching,
        "read failed",
    );
    let error = resolve_scheduler_results(
        "consumer",
        Err(primary),
        "producer",
        Err(typed.clone().into()),
    )
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<axon_api::source::ApiError>(),
        Some(&typed)
    );
}
