use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::*;
use axon_document::DocumentPreparerConfig;

#[tokio::test]
async fn bounded_blocking_map_runs_concurrently_and_preserves_input_order() {
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let observed_active = Arc::clone(&active);
    let observed_maximum = Arc::clone(&maximum);

    let output = bounded_blocking_map_in_order(
        (0_usize..8).collect::<Vec<_>>(),
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
        bounded_blocking_map_in_order(vec![8_usize], 2, 4, |item| *item, Ok),
    )
    .await
    .expect("oversized item must not deadlock")
    .expect("bounded blocking map");

    assert_eq!(output, vec![8]);
}

#[tokio::test]
async fn slow_first_item_does_not_block_replacement_work_and_results_remain_ordered() {
    let gate = Arc::new(std::sync::Barrier::new(2));
    let (completed_tx, mut completed_rx) = tokio::sync::mpsc::unbounded_channel();
    let worker_gate = Arc::clone(&gate);
    let task = tokio::spawn(bounded_blocking_map_in_order(
        vec![0_usize, 1, 2, 3],
        2,
        8,
        |_| 1,
        move |item| {
            if item == 0 {
                worker_gate.wait();
            } else {
                completed_tx.send(item).expect("completion observer");
            }
            Ok(item)
        },
    ));

    let later = tokio::time::timeout(Duration::from_secs(1), async {
        let mut values = Vec::new();
        for _ in 0..3 {
            values.push(completed_rx.recv().await.expect("later completion"));
        }
        values
    })
    .await
    .expect("replacement work must run while item zero is gated");
    assert_eq!(later, vec![1, 2, 3]);
    gate.wait();
    assert_eq!(
        task.await.expect("map task").expect("map result"),
        vec![0, 1, 2, 3]
    );
}

#[tokio::test]
async fn prepare_documents_uses_the_runtime_injected_markdown_limits() {
    let text = format!("# Injected\n{}", "content ".repeat(40));
    let documents = vec![SourceDocument {
        document_id: DocumentId::from("doc-injected"),
        source_id: SourceId::from("source-injected"),
        source_item_key: SourceItemKey::from("item-injected"),
        canonical_uri: "https://example.com/injected".to_string(),
        content_kind: ContentKind::Markdown,
        content: ContentRef::InlineText { text },
        metadata: MetadataMap::new(),
        title: None,
        language: None,
        path: None,
        mime_type: Some("text/markdown".to_string()),
        structured_payload: None,
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    }];
    let preparer = DocumentPreparer::new(DocumentPreparerConfig {
        markdown_max_chars: 48,
        markdown_min_chars: 1,
        markdown_overlap_chars: 0,
    });

    let prepared = prepare_documents(
        documents,
        &SourceGenerationId::from("generation-injected"),
        &BTreeMap::new(),
        preparer,
        1,
        64 * 1024 * 1024,
    )
    .await
    .expect("prepare documents");

    assert!(prepared[0].chunks.len() > 1);
    assert!(
        prepared[0]
            .chunks
            .iter()
            .all(|chunk| chunk.content.chars().count() <= 48)
    );
}
