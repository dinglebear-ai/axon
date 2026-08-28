use super::*;

#[test]
fn prepared_batch_counts_all_chunks() {
    let batch = PreparedGenerationBatch {
        sequence: 7,
        prepared: Vec::new(),
        side_effects: PreparedBatchSideEffects {
            acquisition_artifacts: Vec::new(),
            enrichment_artifacts: Vec::new(),
            clean_output: SourceOutput::default(),
            archive_items: Vec::new(),
            artifact_candidates: Vec::new(),
            warnings: Vec::new(),
            reused_item_keys: Vec::new(),
            refreshed_manifest_items: Vec::new(),
        },
        is_final: true,
    };
    assert_eq!(batch.sequence, 7);
    assert!(batch.is_final);
    assert_eq!(batch.chunk_count(), 0);
}

#[tokio::test]
async fn zero_chunk_message_is_bounded_and_releases_on_drop() {
    let (sender, mut receiver) = prepared_work_channel(4).unwrap();
    let cancel = CancellationToken::new();
    sender
        .send(Vec::new(), empty_side_effects(), &cancel)
        .await
        .unwrap();
    let envelope = receiver.recv().await.unwrap();
    assert_eq!(envelope.sequence, 0);
    assert!(envelope.estimated_bytes > 0);
    assert!(!envelope.is_final);
    drop(envelope);
}

#[tokio::test]
async fn closed_receiver_returns_an_error() {
    let (sender, receiver) = prepared_work_channel(4).unwrap();
    drop(receiver);
    let error = sender
        .send(Vec::new(), empty_side_effects(), &CancellationToken::new())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("receiver closed"));
}

#[tokio::test]
async fn cancellation_interrupts_send() {
    let (sender, _receiver) = prepared_work_channel(4).unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(
        sender
            .send(Vec::new(), empty_side_effects(), &cancel)
            .await
            .is_err()
    );
}

fn empty_side_effects() -> PreparedBatchSideEffects {
    PreparedBatchSideEffects::empty()
}
