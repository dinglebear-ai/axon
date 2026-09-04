//! Binary-content guards for the web adapter.

use axon_api::source::{ApiError, ErrorStage, ManifestItem};

use crate::adapter::Result;

pub(super) fn uri_has_pdf_path(uri: &str) -> bool {
    let path = uri.split(['?', '#']).next().unwrap_or(uri);
    path.to_ascii_lowercase().ends_with(".pdf")
}

pub(super) fn reject_binary_rendered_payload(item: &ManifestItem, content: &str) -> Result<()> {
    let sample = &content.as_bytes()[..content.len().min(4096)];
    let disallowed_controls = sample
        .iter()
        .filter(|byte| **byte < 0x20 && !matches!(**byte, b'\n' | b'\r' | b'\t'))
        .count();
    let looks_binary = sample.starts_with(b"%PDF-")
        || sample.starts_with(b"PK\x03\x04")
        || sample.contains(&0)
        || (!sample.is_empty() && disallowed_controls * 32 >= sample.len());
    if !looks_binary {
        return Ok(());
    }

    Err(ApiError::new(
        "web.render.binary_payload",
        ErrorStage::Fetching,
        "render provider returned binary content as markdown",
    )
    .with_source_id(item.source_id.0.clone())
    .with_context(
        "uri",
        crate::web_engine::engine::url_utils::sanitize_url_for_reporting(&item.canonical_uri),
    )
    .with_context("sample_bytes", sample.len().to_string()))
}
