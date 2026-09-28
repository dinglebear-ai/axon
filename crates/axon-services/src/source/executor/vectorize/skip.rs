//! Partition preparation outcomes while preserving skipped item status.
use super::*;

pub(super) fn partition_prepared(
    outcomes: Vec<axon_document::PrepareSourceDocumentResult>,
) -> (Vec<PreparedDocument>, VectorizeResult) {
    let mut prepared = Vec::new();
    let mut skipped = VectorizeResult::default();
    for outcome in outcomes {
        match outcome {
            axon_document::PrepareSourceDocumentResult::Prepared(document) => {
                prepared.push(document)
            }
            axon_document::PrepareSourceDocumentResult::Skipped(document) => {
                let status = DocumentStatus {
                    document_id: document.document_id,
                    source_id: document.source_id,
                    source_item_key: document.source_item_key.clone(),
                    generation: Some(document.generation),
                    status: DocumentLifecycleStatus::Skipped,
                    updated_at: timestamp(),
                    chunk_count: 0,
                    vector_point_count: 0,
                    error: Some(SourceError {
                        code: document.reason.as_str().to_owned(),
                        message: document.reason.as_str().to_owned(),
                        severity: Severity::Info,
                        provider_id: None,
                        cause: None,
                        source_item_key: Some(document.source_item_key),
                        retryable: false,
                    }),
                    cleanup_status: None,
                };
                skipped.documents_skipped += 1;
                skipped
                    .document_status_positions
                    .insert(status.document_id.clone(), skipped.document_statuses.len());
                skipped.document_statuses.push(status);
            }
        }
    }
    (prepared, skipped)
}
