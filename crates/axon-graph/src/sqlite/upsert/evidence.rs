use axon_api::source::GraphEvidence;
use axon_core::redact::{
    DefaultRedactor, RedactionContext, Redactor, redact_metadata_checked, stamp_redaction_metadata,
};
use sqlx::{QueryBuilder, Sqlite};

use super::StoreResult;
use crate::error::graph_storage_error;
use crate::sqlite::row::{metadata_to_json, range_to_json};

struct EvidenceWrite {
    evidence_id: String,
    edge_id: String,
    evidence_kind: String,
    source_id: String,
    source_item_key: String,
    document_id: Option<String>,
    chunk_id: Option<String>,
    range_json: Option<String>,
    quote: Option<String>,
    confidence: f64,
    metadata_json: String,
}

/// Upsert evidence in bounded multi-row statements. Eleven binds per row keep
/// batches of 80 under SQLite's conservative 999-variable ceiling.
pub(super) async fn upsert_evidence_batch(
    tx: &mut sqlx::SqliteConnection,
    entries: &[(String, &GraphEvidence)],
) -> StoreResult<()> {
    const EVIDENCE_BATCH_SIZE: usize = 80;
    let redactor = DefaultRedactor::new();
    let context = RedactionContext::graph_evidence();
    for entries in entries.chunks(EVIDENCE_BATCH_SIZE) {
        let mut rows = Vec::with_capacity(entries.len());
        for (edge_id, ev) in entries {
            let redacted_quote = ev
                .quote
                .as_ref()
                .map(|quote| redactor.redact_text(quote, &context));
            let mut evidence_metadata = ev.metadata.clone();
            evidence_metadata.insert("source_id".to_string(), serde_json::json!(ev.source_id.0));
            evidence_metadata.insert(
                "source_item_key".to_string(),
                serde_json::json!(ev.source_item_key.0),
            );
            if let Some(document_id) = &ev.document_id {
                evidence_metadata
                    .insert("document_id".to_string(), serde_json::json!(document_id.0));
            }
            if let Some(chunk_id) = &ev.chunk_id {
                evidence_metadata.insert("chunk_id".to_string(), serde_json::json!(chunk_id.0));
            }
            let (redacted_metadata, redaction_report) =
                redact_metadata_checked(evidence_metadata, &context, &redactor)?;
            let redacted_metadata = stamp_redaction_metadata(redacted_metadata, &redaction_report);
            rows.push(EvidenceWrite {
                evidence_id: ev.evidence_id.clone(),
                edge_id: edge_id.clone(),
                evidence_kind: ev.evidence_kind.clone(),
                source_id: ev.source_id.0.clone(),
                source_item_key: ev.source_item_key.0.clone(),
                document_id: ev.document_id.as_ref().map(|d| d.0.clone()),
                chunk_id: ev.chunk_id.as_ref().map(|c| c.0.clone()),
                range_json: range_to_json(&ev.range)?,
                quote: redacted_quote,
                confidence: ev.confidence as f64,
                metadata_json: metadata_to_json(&redacted_metadata)?,
            });
        }
        execute_evidence_batch(tx, &rows).await?;
    }
    Ok(())
}

async fn execute_evidence_batch(
    tx: &mut sqlx::SqliteConnection,
    rows: &[EvidenceWrite],
) -> StoreResult<()> {
    let mut query = QueryBuilder::<Sqlite>::new(
        "INSERT INTO graph_evidence (evidence_id, edge_id, evidence_kind, source_id, \
         source_item_key, document_id, chunk_id, range_json, quote, confidence, metadata_json) ",
    );
    query.push_values(rows, |mut row, value| {
        row.push_bind(&value.evidence_id)
            .push_bind(&value.edge_id)
            .push_bind(&value.evidence_kind)
            .push_bind(&value.source_id)
            .push_bind(&value.source_item_key)
            .push_bind(&value.document_id)
            .push_bind(&value.chunk_id)
            .push_bind(&value.range_json)
            .push_bind(&value.quote)
            .push_bind(value.confidence)
            .push_bind(&value.metadata_json);
    });
    query.push(
        " ON CONFLICT(edge_id, evidence_id) DO UPDATE SET \
         evidence_kind = excluded.evidence_kind, source_id = excluded.source_id, \
         source_item_key = excluded.source_item_key, document_id = excluded.document_id, \
         chunk_id = excluded.chunk_id, range_json = excluded.range_json, \
         quote = excluded.quote, confidence = excluded.confidence, \
         metadata_json = excluded.metadata_json",
    );
    query
        .build()
        .execute(&mut *tx)
        .await
        .map_err(|e| graph_storage_error(format!("failed to batch upsert evidence: {e}")))?;
    Ok(())
}
