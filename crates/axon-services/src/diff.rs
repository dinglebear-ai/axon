//! Diff service: fetch two URLs and compare their content.
//!
//! The pure computation (`compute_diff`) is separated from I/O (`diff`) so it
//! can be tested without network calls.

use std::error::Error;

use tokio::sync::mpsc;

use crate::events::{LogLevel, ServiceEvent, emit};
use crate::scrape;
use crate::types::DiffResult;
pub(crate) use axon_api::diff::{compute_diff, extract_links_from_payload};
use axon_core::config::Config;

/// Fetch `url_a` and `url_b`, then compute and return a `DiffResult`.
pub async fn diff(
    cfg: &Config,
    url_a: &str,
    url_b: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<DiffResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("diff: fetching {url_a} and {url_b}"),
        },
    )
    .await;

    let results =
        scrape::scrape_batch(cfg, &[url_a.to_string(), url_b.to_string()], tx.clone()).await?;

    let (doc_a, doc_b) = match results.as_slice() {
        [a, b] => (a, b),
        _ => {
            return Err("diff requires exactly two URLs to be fetched successfully".into());
        }
    };

    let links_a = extract_links_from_payload(&doc_a.payload);
    let links_b = extract_links_from_payload(&doc_b.payload);

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "diff: computing changes".to_string(),
        },
    )
    .await;

    Ok(compute_diff(
        &doc_a.url,
        &doc_a.markdown,
        &links_a,
        &doc_a.payload,
        &doc_b.url,
        &doc_b.markdown,
        &links_b,
        &doc_b.payload,
    ))
}

/// Pure diff computation — no I/O.
///
/// Exposed as `pub(crate)` so sidecar tests can call it directly without
/// requiring network access.
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
