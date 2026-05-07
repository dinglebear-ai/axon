use crate::core::config::Config;
use crate::core::logging::log_warn;
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::error::Error;

use super::super::client::qdrant_retrieve_by_url;
use super::super::utils::{render_full_doc_from_points, retrieve_max_points};

pub async fn retrieve_result(
    cfg: &Config,
    target: &str,
    max_points: Option<usize>,
) -> Result<(usize, String), Box<dyn Error + Send + Sync>> {
    let max_points = retrieve_max_points(max_points);
    let candidates = crate::vector::ops::input::url_lookup_candidates(target);

    let mut lookups: FuturesUnordered<_> = candidates
        .iter()
        .map(|candidate| qdrant_retrieve_by_url(cfg, candidate, Some(max_points)))
        .collect();

    let mut points = Vec::new();
    let mut had_success = false;
    let mut first_error: Option<String> = None;
    while let Some(result) = lookups.next().await {
        match result {
            Ok(candidate_points) => {
                had_success = true;
                if !candidate_points.is_empty() {
                    points = candidate_points;
                    break;
                }
            }
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(err.to_string());
                }
                log_warn(&format!(
                    "retrieve variant lookup failed for {}: {err}",
                    target
                ));
            }
        }
    }
    if points.is_empty()
        && !had_success
        && let Some(err) = first_error
    {
        return Err(format!("retrieve failed for all URL variants: {err}").into());
    }
    if points.is_empty() {
        return Ok((0, String::new()));
    }
    let chunk_count = points.len();
    let out = render_full_doc_from_points(points);
    Ok((chunk_count, out))
}
