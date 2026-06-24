//! RSS/Atom/JSON feed ingest service wrappers.
//!
//! Split out of `src/services/ingest.rs` to keep that module under the
//! repository's 500-line file cap. Re-exported from the parent module so the
//! public API (`services::crate::ingest_rss[_with_progress]`) is unchanged.

use std::error::Error;

use tokio::sync::mpsc;

use crate::progress::PhaseReporter;
use axon_api::job_dto::IngestResult;
use axon_core::config::Config;
use axon_core::events::{LogLevel, ServiceEvent, emit};

use super::{ingest_payload, map_ingest_result};

/// Ingest an RSS/Atom/JSON feed into the vector store.
///
/// `url` is the feed document URL. Each entry is embedded as one document
/// (HTML content converted to markdown) with title/link/published metadata.
#[must_use = "ingest_rss returns a Result that should be handled"]
pub async fn ingest_rss(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_rss_with_progress(cfg, url, tx, None).await
}

/// Ingest an RSS/Atom/JSON feed with an optional structured progress sink.
#[must_use = "ingest_rss_with_progress returns a Result that should be handled"]
pub async fn ingest_rss_with_progress(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
) -> Result<IngestResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("ingesting rss feed: {url}"),
        },
    )
    .await;

    let reporter = PhaseReporter::new(progress_tx);
    let chunks = crate::rss::ingest_rss(cfg, url, &reporter)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("rss ingest failed for {url}: {e:#}").into() })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("rss ingest complete: {chunks} chunks"),
        },
    )
    .await;

    let payload = ingest_payload("rss", Some(("url", url)), chunks);
    Ok(map_ingest_result(payload))
}
