use crate::core::config::Config;
use crate::core::http::normalize_url;
use crate::core::logging::log_warn;
use std::error::Error;

use super::super::client::qdrant_retrieve_by_url_details;
use super::super::types::{DirectRetrieveResult, RetrieveVariantError};
use super::super::utils::{render_full_doc_from_points, retrieve_max_points};

fn canonical_first_url_candidates(target: &str) -> Vec<String> {
    let normalized = normalize_url(target);
    let trimmed = normalized.trim_end_matches('/').to_string();
    let slash = format!("{}/", trimmed);
    let variants = [normalized.to_string(), trimmed, slash, target.to_string()];
    let mut out = Vec::new();
    for variant in variants {
        if variant.is_empty() || out.contains(&variant) {
            continue;
        }
        out.push(variant);
    }
    out
}

pub async fn retrieve_result(
    cfg: &Config,
    target: &str,
    max_points: Option<usize>,
) -> Result<DirectRetrieveResult, Box<dyn Error + Send + Sync>> {
    let max_points = retrieve_max_points(max_points);
    let candidates = canonical_first_url_candidates(target);
    let mut variant_errors = Vec::new();
    let mut warnings = Vec::new();
    let mut had_success = false;

    for candidate in candidates {
        match qdrant_retrieve_by_url_details(cfg, &candidate, Some(max_points)).await {
            Ok(result) => {
                had_success = true;
                if result.malformed_points > 0 {
                    warnings.push(format!(
                        "{} malformed Qdrant point(s) skipped for URL variant {}",
                        result.malformed_points, candidate
                    ));
                }
                if result.points.is_empty() {
                    continue;
                }
                if result.truncated {
                    warnings.push(format!(
                        "retrieve result truncated at {} point(s) for URL variant {}",
                        result.max_points, candidate
                    ));
                }
                let chunk_count = result.points.len();
                let content = render_full_doc_from_points(result.points);
                return Ok(DirectRetrieveResult {
                    requested_url: target.to_string(),
                    matched_url: Some(result.url_match),
                    chunk_count,
                    content,
                    truncated: result.truncated,
                    warnings,
                    variant_errors,
                });
            }
            Err(err) => {
                log_warn(&format!(
                    "retrieve variant lookup failed for {target} candidate {candidate}: {err}"
                ));
                variant_errors.push(RetrieveVariantError {
                    url: candidate,
                    error: err.to_string(),
                });
            }
        }
    }
    if !had_success {
        let err = variant_errors
            .first()
            .map(|e| e.error.as_str())
            .unwrap_or("no URL variants were available");
        return Err(format!("retrieve failed for all URL variants: {err}").into());
    }
    Ok(DirectRetrieveResult {
        requested_url: target.to_string(),
        matched_url: None,
        chunk_count: 0,
        content: String::new(),
        truncated: false,
        warnings,
        variant_errors,
    })
}

#[cfg(test)]
#[path = "retrieve_tests.rs"]
mod tests;
