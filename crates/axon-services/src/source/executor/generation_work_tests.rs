use super::*;

#[test]
fn resident_estimate_tracks_chunk_payload_without_serializing() {
    let small = prepared_resident_bytes(&[prepared_document(1)]);
    let large = prepared_resident_bytes(&[prepared_document(4)]);
    assert!(small > 0);
    assert!(large > small);
}

#[tokio::test]
async fn zero_chunk_message_is_bounded_and_releases_on_drop() {
    let (mut sender, mut receiver) = prepared_work_channel(4).unwrap();
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
    let (mut sender, receiver) = prepared_work_channel(4).unwrap();
    drop(receiver);
    let error = sender
        .send(Vec::new(), empty_side_effects(), &CancellationToken::new())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("receiver closed"));
}

#[tokio::test]
async fn cancellation_interrupts_send() {
    let (mut sender, mut receiver) = prepared_work_channel(4).unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(
        sender
            .send(Vec::new(), empty_side_effects(), &cancel)
            .await
            .is_err()
    );

    sender
        .send(Vec::new(), empty_side_effects(), &CancellationToken::new())
        .await
        .expect("sender remains usable after pre-send cancellation");
    assert_eq!(receiver.recv().await.unwrap().sequence, 0);
}

#[tokio::test]
async fn concurrent_channels_share_process_budget_and_cancellation_recovers_permits() {
    let shared = Arc::new(Semaphore::new(1));
    let (mut first_sender, mut first_receiver) = prepared_work_channel(4).unwrap();
    let (mut second_sender, _second_receiver) = prepared_work_channel(4).unwrap();
    first_sender.process_byte_permits = Arc::clone(&shared);
    second_sender.process_byte_permits = Arc::clone(&shared);

    first_sender
        .send(Vec::new(), empty_side_effects(), &CancellationToken::new())
        .await
        .expect("first channel acquires shared budget");
    let first = first_receiver.recv().await.expect("first envelope");

    let cancel = CancellationToken::new();
    let cancel_wait = cancel.clone();
    let blocked = tokio::spawn(async move {
        second_sender
            .send(Vec::new(), empty_side_effects(), &cancel_wait)
            .await
            .map(|_| second_sender)
    });
    tokio::task::yield_now().await;
    assert!(
        !blocked.is_finished(),
        "second job must share the process gate"
    );
    cancel.cancel();
    let (mut second_sender, mut recovery_receiver) =
        match blocked.await.expect("blocked sender task") {
            Ok(_) => panic!("cancellation must interrupt shared admission"),
            Err(error) => {
                assert!(error.to_string().contains("canceled"));
                // Recreate the sender after the canceled future consumed it while
                // retaining the exact same process gate for the recovery proof.
                let (mut sender, receiver) = prepared_work_channel(4).unwrap();
                sender.process_byte_permits = Arc::clone(&shared);
                (sender, receiver)
            }
        };

    drop(first);
    second_sender
        .send(Vec::new(), empty_side_effects(), &CancellationToken::new())
        .await
        .expect("canceled acquisition did not leak either byte permit");
    drop(recovery_receiver.recv().await.expect("second envelope"));
    assert_eq!(shared.available_permits(), 1);
}

#[tokio::test]
async fn oversized_batch_splits_losslessly_and_marks_only_last_envelope_final() {
    let (mut sender, mut receiver) = prepared_work_channel(2).unwrap();
    let cancel = CancellationToken::new();
    let send = tokio::spawn({
        let cancel = cancel.clone();
        async move {
            sender
                .send_final(
                    vec![prepared_document(5)],
                    PreparedBatchSideEffects {
                        reused_item_keys: vec![SourceItemKey::new("retained-side-effect")],
                        ..empty_side_effects()
                    },
                    true,
                    &cancel,
                )
                .await
        }
    });

    let mut envelopes = Vec::new();
    for _ in 0..3 {
        envelopes.push(receiver.recv().await.expect("split envelope"));
    }
    send.await.expect("send task").expect("send batch");

    assert_eq!(
        envelopes
            .iter()
            .map(|envelope| envelope.sequence)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(
        envelopes
            .iter()
            .map(|envelope| envelope.prepared[0].chunks.len())
            .collect::<Vec<_>>(),
        vec![2, 2, 1]
    );
    assert_eq!(
        envelopes
            .iter()
            .flat_map(|envelope| &envelope.prepared[0].chunks)
            .map(|chunk| chunk.chunk_id.0.as_str())
            .collect::<Vec<_>>(),
        vec!["chunk-0", "chunk-1", "chunk-2", "chunk-3", "chunk-4"]
    );
    assert_eq!(
        envelopes
            .iter()
            .map(|envelope| envelope.is_final)
            .collect::<Vec<_>>(),
        vec![false, false, true]
    );
    assert_eq!(envelopes[0].side_effects.reused_item_keys.len(), 1);
    assert!(
        envelopes[1..]
            .iter()
            .all(|envelope| envelope.side_effects.reused_item_keys.is_empty())
    );
}

#[tokio::test]
async fn consuming_envelopes_releases_capacity_for_a_blocked_sender() {
    let (mut sender, mut receiver) = prepared_work_channel(1).unwrap();
    let cancel = CancellationToken::new();
    let send = tokio::spawn({
        let cancel = cancel.clone();
        async move {
            sender
                .send(vec![prepared_document(4)], empty_side_effects(), &cancel)
                .await
        }
    });

    let first = receiver.recv().await.expect("first envelope");
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(20),
            &mut Box::pin(async {
                while !send.is_finished() {
                    tokio::task::yield_now().await;
                }
            })
        )
        .await
        .is_err(),
        "the remaining split envelopes should still be backpressured"
    );
    drop(first);
    for _ in 0..3 {
        drop(receiver.recv().await.expect("remaining envelope"));
    }
    send.await.expect("send task").expect("send batch");
}

#[tokio::test]
async fn zero_chunk_documents_retain_per_document_capacity_across_envelopes() {
    let (mut sender, mut receiver) = prepared_work_channel(2).unwrap();
    let cancel = CancellationToken::new();
    let send = tokio::spawn({
        let cancel = cancel.clone();
        async move {
            sender
                .send(
                    (0..8).map(|_| prepared_document(0)).collect(),
                    empty_side_effects(),
                    &cancel,
                )
                .await
        }
    });

    // Three two-document envelopes consume all six charged-document permits.
    // Holding them must backpressure the fourth envelope even though none of
    // the documents contains a vector chunk.
    let mut held = Vec::new();
    for _ in 0..3 {
        held.push(receiver.recv().await.expect("zero-chunk envelope"));
    }
    assert!(held.iter().all(|envelope| envelope.prepared.len() == 2));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), receiver.recv())
            .await
            .is_err(),
        "the fourth envelope must wait for a per-document permit"
    );

    drop(held.remove(0));
    let final_envelope = tokio::time::timeout(std::time::Duration::from_secs(1), receiver.recv())
        .await
        .expect("released permit should unblock sender")
        .expect("fourth zero-chunk envelope");
    assert_eq!(final_envelope.prepared.len(), 2);
    drop(final_envelope);
    drop(held);
    send.await.expect("send task").expect("send batch");
}

fn prepared_document(chunk_count: usize) -> PreparedDocument {
    let document_id = DocumentId::new("doc-generation-work-test");
    PreparedDocument {
        document_id: document_id.clone(),
        source_id: SourceId::new("src-generation-work-test"),
        source_item_key: SourceItemKey::new("item-generation-work-test"),
        generation: SourceGenerationId::new("1"),
        canonical_uri: "memory://generation-work-test".to_string(),
        prepare_version: "test".to_string(),
        chunking_profile: "test".to_string(),
        chunking_method: "test".to_string(),
        chunks: (0..chunk_count)
            .map(|index| PreparedChunk {
                chunk_id: ChunkId::new(format!("chunk-{index}")),
                chunk_key: format!("chunk-{index}"),
                document_id: document_id.clone(),
                chunk_index: index as u32,
                content: format!("chunk {index}"),
                content_hash: format!("hash-{index}"),
                embedding_text: None,
                chunk_locator: ChunkLocator {
                    canonical_uri: "memory://generation-work-test".to_string(),
                    path: None,
                    heading_path: Vec::new(),
                    symbol: None,
                    range: empty_range(),
                },
                source_range: empty_range(),
                content_kind: ContentKind::Markdown,
                title: None,
                graph_refs: Vec::new(),
                parent_chunk_id: None,
                previous_chunk_id: None,
                next_chunk_id: None,
                metadata: MetadataMap::new(),
            })
            .collect(),
        metadata: MetadataMap::new(),
        cleanup_keys: Vec::new(),
        graph_refs: Vec::new(),
        parse_facts: Vec::new(),
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    }
}

fn empty_range() -> SourceRange {
    SourceRange {
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        char_start: None,
        char_end: None,
        time_start_ms: None,
        time_end_ms: None,
        dom_selector: None,
        json_pointer: None,
        yaml_path: None,
        xml_xpath: None,
        csv_row: None,
        session_turn_id: None,
        turn_start: None,
        turn_end: None,
    }
}

fn empty_side_effects() -> PreparedBatchSideEffects {
    PreparedBatchSideEffects::empty()
}
