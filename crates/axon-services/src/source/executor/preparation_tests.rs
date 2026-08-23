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
