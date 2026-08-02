//! HTTP-backed web acquisition and conditional reuse responses.

use axon_api::source::*;
use axon_core::logging::log_info;
use serde_json::Value;

use crate::adapter::Result;
use crate::boundary::FetchProvider;

/// HTTP-mode acquisition. A conditional 304 response returns a sentinel item
/// so the services layer can reuse the previous committed representation or
/// perform an unconditional refetch before publication.
pub(crate) async fn acquire_via_fetch(
    fetch: &dyn FetchProvider,
    item: &ManifestItem,
    cache_policy: CachePolicy,
    headers: &[RedactedHeader],
) -> Result<Option<AcquiredSourceItem>> {
    let (prior_etag, prior_last_modified) = prior_validators(item, cache_policy);
    let sent_prior_validator = prior_etag.is_some() || prior_last_modified.is_some();
    let fetched = fetch
        .fetch(build_fetch_request(
            item,
            prior_etag,
            prior_last_modified,
            headers,
        ))
        .await?;
    if fetched.status == 304 {
        return not_modified_item(
            item,
            cache_policy,
            prior_etag,
            prior_last_modified,
            sent_prior_validator,
            fetched,
        );
    }
    Ok(Some(acquired_from_fetched(
        item,
        fetched,
        prior_etag,
        prior_last_modified,
    )))
}

fn prior_validators(
    item: &ManifestItem,
    cache_policy: CachePolicy,
) -> (Option<&str>, Option<&str>) {
    if cache_policy != CachePolicy::Revalidate {
        return (None, None);
    }
    (
        item.metadata.get("web_prior_etag").and_then(Value::as_str),
        item.metadata
            .get("web_prior_last_modified")
            .and_then(Value::as_str),
    )
}

fn not_modified_item(
    item: &ManifestItem,
    cache_policy: CachePolicy,
    prior_etag: Option<&str>,
    prior_last_modified: Option<&str>,
    sent_prior_validator: bool,
    fetched: FetchedResource,
) -> Result<Option<AcquiredSourceItem>> {
    if !sent_prior_validator {
        return Err(invalid_unconditional_304(item, cache_policy));
    }
    let mut metadata = MetadataMap::new();
    metadata.insert(
        "web_fetch_method".to_string(),
        serde_json::json!("http_fetch_reuse"),
    );
    metadata.insert("web_render_mode".to_string(), serde_json::json!("http"));
    metadata.insert("web_status".to_string(), serde_json::json!(304));
    metadata.insert("web_reuse_required".to_string(), serde_json::json!(true));
    if let Some(etag) = prior_etag {
        metadata.insert("web_etag".to_string(), serde_json::json!(etag));
    }
    if let Some(last_modified) = prior_last_modified {
        metadata.insert(
            "web_last_modified".to_string(),
            serde_json::json!(last_modified),
        );
    }
    log_info(&format!(
        "web_etag_conditional: 304 Not Modified for {} — reusing prior committed content if available",
        item.canonical_uri,
    ));
    Ok(Some(AcquiredSourceItem {
        manifest_item: item.clone(),
        fetch_status: LifecycleStatus::Completed,
        content_ref: ContentRef::External {
            uri: format!("reuse://{}", item.source_item_key.0),
            integrity: item.content_hash.clone(),
        },
        raw_artifact_id: None,
        headers: fetched.headers,
        fetched_at: fetched.fetched_at,
        metadata,
    }))
}

fn invalid_unconditional_304(item: &ManifestItem, cache_policy: CachePolicy) -> ApiError {
    ApiError::new(
        "web.fetch.invalid_304_without_validator",
        ErrorStage::Fetching,
        format!(
            "received 304 Not Modified for {} without sending a prior validator",
            item.canonical_uri
        ),
    )
    .with_source_id(item.source_id.0.clone())
    .with_context("uri", item.canonical_uri.clone())
    .with_context("cache_policy", format!("{cache_policy:?}").to_lowercase())
    .with_context(
        "has_web_prior_etag",
        item.metadata.contains_key("web_prior_etag").to_string(),
    )
    .with_context(
        "has_web_prior_last_modified",
        item.metadata
            .contains_key("web_prior_last_modified")
            .to_string(),
    )
}

fn acquired_from_fetched(
    item: &ManifestItem,
    fetched: FetchedResource,
    prior_etag: Option<&str>,
    prior_last_modified: Option<&str>,
) -> AcquiredSourceItem {
    let mut manifest_item = item.clone();
    manifest_item.content_kind = Some(content_kind_for_fetch(&fetched));
    let mut metadata = MetadataMap::new();
    metadata.insert(
        "web_fetch_method".to_string(),
        serde_json::json!("http_fetch"),
    );
    metadata.insert("web_render_mode".to_string(), serde_json::json!("http"));
    metadata.insert("web_status".to_string(), serde_json::json!(fetched.status));
    if let Some(etag) = fetched.etag.as_deref().or(prior_etag) {
        metadata.insert("web_etag".to_string(), serde_json::json!(etag));
    }
    if let Some(last_modified) = header_value(&fetched.headers, "Last-Modified")
        .as_deref()
        .or(prior_last_modified)
    {
        metadata.insert(
            "web_last_modified".to_string(),
            serde_json::json!(last_modified),
        );
    }
    AcquiredSourceItem {
        manifest_item,
        fetch_status: LifecycleStatus::Completed,
        content_ref: fetched.content,
        raw_artifact_id: None,
        headers: fetched.headers,
        fetched_at: fetched.fetched_at,
        metadata,
    }
}

/// Build a GET request and append validated conditional headers when prior
/// committed validators are available.
pub(super) fn build_fetch_request(
    item: &ManifestItem,
    prior_etag: Option<&str>,
    prior_last_modified: Option<&str>,
    headers: &[RedactedHeader],
) -> FetchRequest {
    let mut headers = headers.to_vec();
    if let Some(etag) = prior_etag {
        headers.push(RedactedHeader {
            name: "If-None-Match".to_string(),
            value: etag.to_string(),
            redacted: false,
        });
    }
    if let Some(last_modified) = prior_last_modified {
        headers.push(RedactedHeader {
            name: "If-Modified-Since".to_string(),
            value: last_modified.to_string(),
            redacted: false,
        });
    }
    FetchRequest {
        uri: item.canonical_uri.clone(),
        method: "GET".to_string(),
        headers: RedactedHeaders { headers },
        body: None,
        timeout_ms: None,
        max_bytes: None,
        credential_refs: Vec::new(),
        metadata: MetadataMap::new(),
    }
}

fn header_value(headers: &RedactedHeaders, name: &str) -> Option<String> {
    headers
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.clone())
}

fn content_kind_for_fetch(fetched: &FetchedResource) -> ContentKind {
    if matches!(fetched.content, ContentRef::InlineBytes { .. }) {
        return ContentKind::BinaryMetadata;
    }
    let content_type = fetched
        .headers
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-type"))
        .map(|header| header.value.to_ascii_lowercase())
        .unwrap_or_default();
    if content_type.contains("json") {
        ContentKind::Json
    } else if content_type.contains("xml") {
        ContentKind::Xml
    } else if content_type.contains("markdown") {
        ContentKind::Markdown
    } else if content_type.contains("text/plain") {
        ContentKind::PlainText
    } else {
        ContentKind::Html
    }
}
