//! Group collected [`super::SessionDoc`]s by collection and embed+upsert each
//! group through the ad-hoc contract write path
//! (`crate::contract_write::embed_and_upsert_documents`).
//!
//! Split out of `sessions_legacy.rs` to keep that file under the repo's
//! 500-line monolith cap.

use std::collections::HashMap;
use std::error::Error;

use axon_api::source::PreparedDocument;
use axon_core::config::Config;
use axon_core::logging::log_warn;

use crate::contract_write::{self, ContractWriteSummary};

use super::SessionDoc;

/// Groups collected docs by collection and embeds+upserts once per collection
/// (`contract_write::embed_and_upsert_documents`).
pub(super) async fn embed_all_session_docs(cfg: &Config, docs: Vec<SessionDoc>) -> usize {
    match embed_session_docs(cfg, docs, false).await {
        Ok(total) => total,
        Err(error) => {
            log_warn(&format!("sessions embed failed: {error}"));
            0
        }
    }
}

pub(super) async fn embed_session_docs(
    cfg: &Config,
    docs: Vec<SessionDoc>,
    strict: bool,
) -> Result<usize, Box<dyn Error>> {
    let mut by_collection: HashMap<String, Vec<SessionDoc>> = HashMap::new();
    for sd in docs {
        by_collection.entry(sd.collection.clone()).or_default().push(sd);
    }

    let mut total = 0;
    for (collection, session_docs) in by_collection {
        let doc_count = session_docs.len();
        let (prepared, prep_failed) = prepare_session_docs(session_docs, strict)?;

        match contract_write::embed_and_upsert_documents(cfg, &collection, prepared).await {
            Ok(summary) => {
                let chunks_embedded = embedded_chunks_after_prep_failures(
                    summary,
                    prep_failed,
                    strict,
                    &collection,
                )?;
                total += chunks_embedded;
                if strict && doc_count > 0 && chunks_embedded == 0 {
                    return Err(format!(
                        "sessions embed produced zero chunks for nonempty collection={collection}"
                    )
                    .into());
                }
            }
            Err(e) => {
                let message = format!("sessions embed failed collection={collection} error={e}");
                if strict {
                    return Err(message.into());
                }
                log_warn(&message);
            }
        }
    }
    Ok(total)
}

/// Prepare every `SessionDoc` (chunk it via `DocumentPreparer`), tolerating
/// per-document preparation failures in non-strict mode (matching the legacy
/// `EmbedSummary.docs_failed` partial-failure tolerance). Strict mode (the
/// `/v1/ingest/sessions/prepared` path) fails fast on the first error.
fn prepare_session_docs(
    docs: Vec<SessionDoc>,
    strict: bool,
) -> Result<(Vec<PreparedDocument>, usize), Box<dyn Error>> {
    let mut prepared = Vec::with_capacity(docs.len());
    let mut prep_failed = 0usize;
    for doc in &docs {
        match doc.to_prepared_document() {
            Ok(document) => prepared.push(document),
            Err(err) => {
                let message = format!("sessions prepare failed url={} error={err}", doc.url);
                if strict {
                    return Err(message.into());
                }
                log_warn(&message);
                prep_failed += 1;
            }
        }
    }
    Ok((prepared, prep_failed))
}

fn embedded_chunks_after_prep_failures(
    summary: ContractWriteSummary,
    prep_failed: usize,
    strict: bool,
    collection: &str,
) -> Result<usize, Box<dyn Error>> {
    if prep_failed > 0 {
        let message = format!(
            "sessions embed partial failure collection={collection} docs_failed={prep_failed} docs_embedded={}",
            summary.docs_embedded
        );
        if strict {
            return Err(message.into());
        }
        log_warn(&message);
    }
    Ok(summary.chunks_embedded)
}
