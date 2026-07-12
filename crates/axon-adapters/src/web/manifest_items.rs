//! `ManifestItem` construction shared by the `Map`/`Page`/`Site`/`Docs`
//! discover paths.

use axon_api::source::*;
use serde_json::Value;

use crate::adapter::Result;
use crate::manifest::item_identity;

use super::metadata::web_metadata;
use super::url_parts::WebUrlParts;

/// `Map` scope: the manifest is exactly the caller-supplied `map_urls`
/// acquisition results — no network access, no content acquired later
/// (`acquire` short-circuits to zero fetched items for this scope).
pub(super) fn map_manifest_items(plan: &SourcePlan) -> Result<Vec<ManifestItem>> {
    let urls = plan
        .route
        .validated_options
        .values
        .get("map_urls")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ApiError::new(
                "adapter.web.map_urls.required",
                axon_error::ErrorStage::Discovering,
                "web map scope requires map_urls acquisition results",
            )
        })?;
    let mut items = Vec::with_capacity(urls.len());
    for url in urls {
        let raw = url.as_str().ok_or_else(|| {
            ApiError::new(
                "adapter.web.map_url.invalid",
                axon_error::ErrorStage::Discovering,
                "map_urls entries must be strings",
            )
        })?;
        let web = WebUrlParts::parse(raw)?;
        items.push(web_manifest_item(plan, &web, None, None, None));
    }
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));
    Ok(items)
}

/// `Page` scope: the manifest is trivially the one requested URL. No network
/// access at discover time — `acquire` is the sole fetch for this scope. This
/// means the manifest item carries no `content_hash` until after acquisition,
/// so ledger diffing cannot distinguish "unchanged" from "never acquired" for
/// a `Page` source purely from `discover` output (see `crates/axon-adapters/
/// src/CLAUDE.md`/issue #298 Wave 2 follow-ups).
pub(super) fn page_manifest_item(plan: &SourcePlan) -> Result<ManifestItem> {
    let web = WebUrlParts::parse(&plan.route.source.canonical_uri)?;
    Ok(web_manifest_item(plan, &web, None, None, None))
}

/// Build one `ManifestItem` for a (possibly not-yet-fetched) web URL.
///
/// `content_kind` is intentionally left `None` here — it is only known once
/// `acquire` decides how the item was actually fetched (raw HTTP body vs.
/// rendered markdown); see `web/acquire.rs`.
pub(super) fn web_manifest_item(
    plan: &SourcePlan,
    web: &WebUrlParts,
    content_hash: Option<String>,
    size_bytes: Option<u64>,
    structured: Option<Value>,
) -> ManifestItem {
    let identity = item_identity(SourceKind::Web, "", &web.item_key)
        .expect("web item key is derived from a validated URL");
    let mut metadata = web_metadata(plan, web);
    metadata.insert("content_hash".to_string(), serde_json::json!(content_hash));
    if let Some(structured) =
        structured.and_then(|payload| bounded_structured_payload(payload, &mut metadata))
    {
        metadata.insert("structured_payload".to_string(), structured);
    }
    ManifestItem {
        source_id: plan.route.source.source_id.clone(),
        source_item_key: identity.source_item_key,
        canonical_uri: web.normalized_url.clone(),
        item_kind: ItemKind::WebPage,
        content_kind: None,
        display_path: Some(web.item_key.clone()),
        parent_key: None,
        size_bytes,
        content_hash,
        mtime: None,
        version: None,
        fetch_plan: None,
        metadata,
        graph_hints: Vec::new(),
    }
}

pub(super) fn bounded_structured_payload(
    structured: Value,
    metadata: &mut MetadataMap,
) -> Option<Value> {
    const MAX_STRUCTURED_PAYLOAD_BYTES: usize = 64 * 1024;
    let size = serde_json::to_vec(&structured)
        .map(|bytes| bytes.len())
        .unwrap_or(MAX_STRUCTURED_PAYLOAD_BYTES + 1);
    if size <= MAX_STRUCTURED_PAYLOAD_BYTES {
        Some(structured)
    } else {
        metadata.insert(
            "structured_payload_omitted".to_string(),
            serde_json::json!("too_large"),
        );
        None
    }
}
