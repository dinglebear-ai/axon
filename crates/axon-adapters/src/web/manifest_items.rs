//! `ManifestItem` construction shared by the `Map`/`Page`/`Site`/`Docs`
//! discover paths.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use sha2::{Digest, Sha256};

use axon_api::source::*;
use serde_json::Value;

use crate::adapter::Result;
use crate::boundary::FetchProvider;
use crate::manifest::item_identity;

use super::metadata::web_metadata;
use super::options::{cache_policy, headers};
use super::url_parts::WebUrlParts;

/// `Map` scope: the manifest is exactly the caller-supplied `map_urls`
/// acquisition results — no network access, no content acquired later
/// (`acquire` short-circuits to zero fetched items for this scope).
pub(super) fn map_urls_manifest_items(plan: &SourcePlan) -> Result<Vec<ManifestItem>> {
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

/// `Page` scope: the manifest is the one requested URL, fetched once here so
/// the manifest item carries a real `content_hash` — without it,
/// `ledger.diff_manifest` sees `None == None` across every discover
/// generation and a changed page is never reclassified as `modified` (issue
/// #298 Wave 2 regression fix). `acquire` independently re-fetches this same
/// URL for any item the diff marks `added`/`modified` (see `web/acquire.rs`);
/// re-fetching here is a deliberate "correctness over one extra request"
/// tradeoff, matching the `Site`/`Docs` discover path's own crawl-then-acquire
/// double fetch (see `web/site_discovery.rs`).
pub(super) async fn page_manifest_item(
    plan: &SourcePlan,
    fetch: &dyn FetchProvider,
) -> Result<ManifestItem> {
    let web = WebUrlParts::parse(&plan.route.source.canonical_uri)?;
    let fetched = fetch
        .fetch(build_discover_fetch_request(
            &web,
            headers(&plan.route.validated_options.values),
        ))
        .await?;
    let content_hash = Some(content_ref_hash(&fetched.content));
    let mut item = web_manifest_item(plan, &web, content_hash, fetched.bytes, None);
    attach_conditional_metadata(&mut item, fetched.etag.as_deref());
    Ok(item)
}

fn build_discover_fetch_request(web: &WebUrlParts, headers: Vec<RedactedHeader>) -> FetchRequest {
    FetchRequest {
        uri: web.normalized_url.clone(),
        method: "GET".to_string(),
        headers: RedactedHeaders { headers },
        body: None,
        timeout_ms: None,
        max_bytes: None,
        credential_refs: Vec::new(),
        metadata: MetadataMap::new(),
    }
}

/// Hash a fetched page's raw content bytes with the same SHA-256 hex-digest
/// convention the `Site`/`Docs` crawl engine uses for its manifest entries
/// (`web_engine::engine::collector::page::process_page`), so `content_hash`
/// values are comparable in format across every web discover path.
fn content_ref_hash(content: &ContentRef) -> String {
    let mut hasher = Sha256::new();
    match content {
        ContentRef::InlineText { text } => hasher.update(text.as_bytes()),
        ContentRef::InlineBytes { bytes_base64, .. } => {
            match BASE64_STANDARD.decode(bytes_base64) {
                Ok(bytes) => hasher.update(&bytes),
                Err(_) => hasher.update(bytes_base64.as_bytes()),
            }
        }
        ContentRef::Artifact { artifact_id } => hasher.update(artifact_id.0.as_bytes()),
        ContentRef::External { uri, integrity } => {
            hasher.update(uri.as_bytes());
            if let Some(integrity) = integrity {
                hasher.update(integrity.as_bytes());
            }
        }
    }
    hex::encode(hasher.finalize())
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
        fetch_plan: Some(FetchPlan {
            uri: web.normalized_url.clone(),
            method: "GET".to_string(),
            headers: RedactedHeaders {
                headers: headers(&plan.route.validated_options.values)
                    .into_iter()
                    .map(|header| RedactedHeader {
                        name: header.name,
                        value: "[REDACTED]".to_string(),
                        redacted: true,
                    })
                    .collect(),
            },
            render_required: false,
            cache_policy: cache_policy(&plan.route.validated_options.values),
        }),
        metadata,
        graph_hints: Vec::new(),
    }
}

pub(crate) fn attach_conditional_metadata(item: &mut ManifestItem, etag: Option<&str>) {
    if let Some(etag) = etag {
        item.metadata
            .insert("web_etag".to_string(), serde_json::json!(etag));
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
