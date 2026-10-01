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
async fn bounded_blocking_map_rejects_oversized_item_without_deadlock() {
    let output = tokio::time::timeout(
        Duration::from_secs(1),
        bounded_blocking_map_in_order(vec![8_usize], 2, 4, |item| *item, Ok),
    )
    .await
    .expect("oversized item must not deadlock")
    .expect_err("overweight item must be rejected");

    assert!(output.to_string().contains("exceeds resident byte budget"));
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
        max_content_bytes: axon_document::content_policy::DEFAULT_CONTENT_BYTE_LIMIT,
        markdown_max_chars: 96,
        markdown_min_chars: 1,
        markdown_overlap_chars: 0,
        minimum_chunk_chars: 50,
    });

    let prepared = prepare_documents(
        documents,
        &SourceGenerationId::from("generation-injected"),
        &BTreeMap::new(),
        preparer,
        1,
        64 * 1024 * 1024,
        None,
    )
    .await
    .expect("prepare documents");

    let PrepareSourceDocumentResult::Prepared(document) = &prepared.outcomes[0] else {
        panic!("text skipped")
    };
    assert!(document.chunks.len() > 1);
    assert!(
        document
            .chunks
            .iter()
            .all(|chunk| chunk.content.chars().count() <= 96)
    );
}

#[tokio::test]
async fn zero_preparation_budget_starts_no_work() {
    let started = Arc::new(AtomicUsize::new(0));
    let observed = started.clone();
    let result = bounded_blocking_map_in_order(
        vec![0],
        1,
        0,
        |_| 0,
        move |item| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(item)
        },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(started.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn request_zero_content_limit_produces_explicit_size_skip() {
    let document = preparation_test_document(ContentRef::InlineText {
        text: "text".into(),
    });
    let output = prepare_documents(
        vec![document],
        &SourceGenerationId::from("generation-limit"),
        &BTreeMap::new(),
        crate::test_support::document_preparer_for_small_fixtures(),
        1,
        1024,
        Some(0),
    )
    .await
    .unwrap();
    assert!(
        matches!(&output.outcomes[0], PrepareSourceDocumentResult::Skipped(skipped) if skipped.reason == ContentSkipReason::SizeLimitExceeded)
    );
}

#[test]
fn raw_body_admission_covers_transport_decode_and_utf8_expansion() {
    let document = preparation_test_document(ContentRef::InlineBytes {
        bytes_base64: "AAAAAAAA".into(),
        mime_type: "application/octet-stream".into(),
    });
    // Eight transport bytes, six decoded bytes, twelve output bytes.
    assert!(source_document_bytes(&document) >= 8 + 6 + 12);
}

fn preparation_test_document(content: ContentRef) -> SourceDocument {
    SourceDocument {
        document_id: DocumentId::from("doc-limit"),
        source_id: SourceId::from("source-limit"),
        source_item_key: SourceItemKey::from("item-limit"),
        canonical_uri: "local://limit".into(),
        content_kind: ContentKind::PlainText,
        content,
        metadata: MetadataMap::new(),
        title: None,
        language: None,
        path: None,
        mime_type: None,
        structured_payload: None,
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    }
}

#[tokio::test]
async fn preparation_error_retains_stage_and_item_identity() {
    let document = preparation_test_document(ContentRef::InlineBytes {
        bytes_base64: "not-base64!".into(),
        mime_type: "application/octet-stream".into(),
    });
    let error = prepare_documents(
        vec![document],
        &SourceGenerationId::from("gen_error"),
        &BTreeMap::new(),
        crate::test_support::document_preparer_for_small_fixtures(),
        1,
        4096,
        None,
    )
    .await
    .unwrap_err();
    let typed = error
        .downcast_ref::<ApiError>()
        .expect("structured preparation failure");
    assert_eq!(typed.code.0, "document.prepare_failed");
    assert_eq!(typed.stage, ErrorStage::Preparing);
    assert_eq!(typed.source_item_key.as_deref(), Some("item-limit"));
}

#[tokio::test]
async fn preparation_summary_counts_zero_symbol_ast_even_when_quality_skips() {
    let mut document = preparation_test_document(ContentRef::InlineText {
        text: "let x = 1;".into(),
    });
    document.content_kind = ContentKind::Code;
    document.path = Some("empty.rs".into());
    let batch = prepare_documents(
        vec![document],
        &SourceGenerationId::from("gen-observed"),
        &BTreeMap::new(),
        crate::test_support::document_preparer_for_small_fixtures(),
        1,
        4096,
        None,
    )
    .await
    .unwrap();
    assert!(matches!(
        batch.outcomes[0],
        PrepareSourceDocumentResult::Skipped(_)
    ));
    assert_eq!(batch.summary.ast_zero_symbol, 1);
    assert_eq!(batch.summary.skipped, 1);
    assert_eq!(batch.summary.not_observed, 0);
    assert!(batch.summary.message().contains("supported_attempts=1"));
    assert!(batch.summary.chunk_methods.is_empty());
}

#[test]
fn preparation_summary_excludes_unsupported_from_failure_denominator() {
    use axon_document::PreparationObservation;
    let result = PrepareSourceDocumentResult::Skipped(SkippedDocument {
        document_id: DocumentId::from("doc"),
        source_id: SourceId::from("src"),
        source_item_key: SourceItemKey::from("item"),
        generation: SourceGenerationId::from("gen"),
        reason: ContentSkipReason::EmptyContent,
    });
    let mut summary = PreparationSummary::default();
    for (status, count, extraction) in [
        ("parsed", 1, "ast"),
        ("partial", 1, "ast"),
        ("partial", 0, "ast"),
        ("parsed", 0, "ast"),
        ("unsupported", 1, "heuristic_fallback"),
        ("failed", 1, "heuristic_fallback"),
    ] {
        summary.observe(&result, &PreparationObservation::from_value(&serde_json::json!({
            "code_ast_status": status, "code_symbol_count": count, "symbol_extraction_status": extraction,
            "code_grammar": if matches!(status, "parsed" | "partial") { Some("rust") } else { None }
        })));
    }
    assert_eq!(
        (
            summary.ast_clean,
            summary.ast_recovered,
            summary.ast_zero_symbol
        ),
        (1, 1, 1)
    );
    assert_eq!(
        (
            summary.unsupported_grammar,
            summary.parse_failure,
            summary.heuristic_fallback
        ),
        (1, 1, 2)
    );
    assert_eq!(summary.ast_partial, 1);
    assert!(summary.message().contains("supported_attempts=5"));
    assert!(!summary.message().contains("supported_attempts=6"));
}

#[tokio::test]
async fn preparation_summary_counts_actual_chunk_methods_separately_from_parser_outcomes() {
    let mut document = preparation_test_document(ContentRef::InlineText {
        text: "// This module has no declarations and retains enough meaningful context for search preparation.\n".into(),
    });
    document.content_kind = ContentKind::Code;
    document.path = Some("module.rs".into());
    let batch = prepare_documents(
        vec![document],
        &SourceGenerationId::from("gen-methods"),
        &BTreeMap::new(),
        crate::test_support::document_preparer_for_small_fixtures(),
        1,
        4096,
        None,
    )
    .await
    .unwrap();
    assert_eq!(batch.summary.ast_zero_symbol, 1);
    assert_eq!(batch.summary.chunk_methods.get("tree_sitter"), Some(&1));
    assert_eq!(batch.summary.skipped, 0);
}
