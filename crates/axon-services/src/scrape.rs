use crate::events::{LogLevel, ServiceEvent, emit};
use crate::source::dispatch::{family_source_plan, web_crawl_options};
use crate::types::ScrapeResult;
use axon_adapters::web::WebSourceAdapter;
use axon_api::result::DocumentBackend;
use axon_api::source::{
    AdapterOptions, ArtifactKind, ContentKind, ContentRef, MetadataMap, SourceDocument, SourceKind,
    SourceRequest, SourceScope,
};
use axon_core::boundary::{ArtifactBytesWriteRequest, ArtifactStore, FileArtifactStore};
use axon_core::config::{Config, ScrapeFormat};
use axon_core::content::{
    build_selector_config, extract_anchor_hrefs, extract_meta_description, find_between,
    to_llm_text, to_markdown,
};
use axon_core::http::normalize_url;
use futures_util::stream::{self, StreamExt};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::time::Duration;
use tokio::sync::mpsc;

pub use axon_adapters::web::map_scrape_payload;

pub const MAX_PUBLIC_STRUCTURED_BYTES: usize = 16 * 1024;

/// Scrape a single URL and return a typed [`ScrapeResult`].
///
/// Runs the restored vertical-extractor catalog first when
/// `cfg.enable_verticals` is true, then falls back to the generic HTTP/Chrome
/// scrape path when no extractor claims the URL or automatic extraction
/// degrades. This keeps single-page `scrape` aligned with the unified web
/// source adapter's vertical acquisition behavior.
///
/// `tx` is an optional progress channel. Pass `None` when progress events are
/// not needed (CLI) or `Some(sender)` when the caller wants to observe
/// start/complete log events (web / MCP streaming paths).
#[must_use = "scrape returns a Result that should be handled"]
pub async fn scrape(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<ScrapeResult, Box<dyn Error>> {
    let normalized = validate_and_normalize_scrape_url(url, &tx).await?;
    let document = scrape_document_via_adapter(cfg, &normalized).await?;
    let mut result = scrape_result_from_document(cfg, document)?;
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("scrape complete: {normalized}"),
        },
    )
    .await;
    // Service-side artifact write: if output_path is configured, write atomically
    // so all callers (CLI, MCP, /v1/actions) share identical write semantics.
    if let Some(output_path) = cfg.output_path.as_ref() {
        axon_core::artifacts::atomic_write_explicit(output_path, result.output.as_bytes())
            .await
            .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?;
        let mut metadata = MetadataMap::new();
        metadata.insert("source_url".to_string(), normalized.to_string().into());
        metadata.insert("label".to_string(), "scrape.md".into());
        metadata.insert(
            "line_count".to_string(),
            result.output.lines().count().into(),
        );
        result.artifact_handle = Some(
            FileArtifactStore::new(cfg.output_dir.join("artifacts"))
                .put_bytes(ArtifactBytesWriteRequest {
                    kind: ArtifactKind::NormalizedContent,
                    content_type: "text/markdown".to_string(),
                    bytes: result.output.as_bytes().to_vec(),
                    source_id: None,
                    job_id: None,
                    metadata,
                })
                .await
                .map_err(|error| -> Box<dyn Error> { Box::new(error) })?,
        );
    }
    Ok(result)
}

async fn scrape_document_via_adapter(
    cfg: &Config,
    normalized: &str,
) -> Result<SourceDocument, Box<dyn Error>> {
    let mut request = SourceRequest::new(normalized.to_string());
    request.scope = Some(SourceScope::Page);
    request.embed = false;
    let mut routed = crate::source::routing::resolve_source_route(&request)
        .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;
    if routed.kind != SourceKind::Web {
        return Err(format!(
            "scrape requires a web source; routed {:?} for {normalized}",
            routed.kind
        )
        .into());
    }
    routed.route.validated_options = AdapterOptions {
        values: web_crawl_options(cfg, Some(1), Some(0)),
    };
    let mut plan = family_source_plan(normalized, &routed.route, false, None, None);
    plan.job_id = axon_api::source::JobId::new(uuid::Uuid::new_v4());
    WebSourceAdapter::from_config(cfg)
        .scrape_document(&plan)
        .await
        .map_err(|error| -> Box<dyn Error> { Box::new(error) })
}

fn scrape_result_from_document(
    cfg: &Config,
    document: SourceDocument,
) -> Result<ScrapeResult, Box<dyn Error>> {
    let status_code = document
        .metadata
        .get("web_status")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(200);
    let extractor_name = document
        .metadata
        .get("extractor_name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let extractor_version = document
        .metadata
        .get("extractor_version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let mut title = document.title.clone().or_else(|| {
        document
            .metadata
            .get("web_title")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    });
    let url = document.canonical_uri.clone();
    let content_kind = document.content_kind;
    let text = match document.content {
        ContentRef::InlineText { text } => text,
        ContentRef::InlineBytes { .. } => {
            return Err("scrape projection cannot expose binary page content".into());
        }
        ContentRef::Artifact { .. } | ContentRef::External { .. } => {
            return Err("scrape projection requires inline page content".into());
        }
    };

    let selector = build_selector_config(cfg);
    let (markdown, description, links) = if content_kind == ContentKind::Html {
        if title.is_none() {
            title = find_between(&text, "<title>", "</title>").map(str::to_string);
        }
        let links = extract_anchor_hrefs(&url, &text, 512)
            .into_iter()
            .map(|href| serde_json::json!({ "href": href, "text": "" }))
            .collect::<Vec<_>>();
        (
            to_markdown(&text, selector.as_ref()),
            extract_meta_description(&text),
            links,
        )
    } else {
        (text.clone(), None, extract_markdown_links(&text))
    };

    let payload = serde_json::json!({
        "url": url,
        "status_code": status_code,
        "markdown": markdown,
        "title": title.clone().unwrap_or_default(),
        "description": description.unwrap_or_default(),
        "links": links,
    });
    let output = match cfg.format {
        ScrapeFormat::Markdown => markdown.clone(),
        ScrapeFormat::Html | ScrapeFormat::RawHtml => text,
        ScrapeFormat::Json => serde_json::to_string_pretty(&payload)?,
        ScrapeFormat::Llm => to_llm_text(&markdown, &url),
    };

    let (follow_crawl_urls, extra, structured, structured_for_embedding) =
        structured_scrape_projection(
            document.structured_payload,
            extractor_name.as_deref(),
            extractor_version.as_deref(),
        );

    Ok(ScrapeResult {
        payload,
        url,
        markdown,
        output,
        artifact_handle: None,
        truncated: false,
        token_estimate: None,
        next_cursor: None,
        remaining_tokens_estimate: None,
        backend: Some(DocumentBackend::LiveScrape),
        follow_crawl_urls,
        extra,
        structured,
        structured_for_embedding,
        extractor_name,
        title,
    })
}

fn structured_scrape_projection(
    structured_payload: Option<serde_json::Value>,
    extractor_name: Option<&str>,
    extractor_version: Option<&str>,
) -> (
    Vec<String>,
    Option<serde_json::Value>,
    Option<serde_json::Value>,
    Option<serde_json::Value>,
) {
    let wrapper = structured_payload.map(redact_sensitive_structured_keys);
    let follow_crawl_urls = wrapper
        .as_ref()
        .and_then(|value| value.get("follow_crawl_urls"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flat_map(|items| items.iter())
        .filter_map(serde_json::Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let structured_for_embedding = wrapper.as_ref().and_then(|value| {
        value.get("structured").cloned().or_else(|| {
            (!value
                .get("kind")
                .is_some_and(|kind| kind == "vertical_extractor"))
            .then(|| value.clone())
        })
    });
    let structured = structured_for_embedding
        .clone()
        .and_then(capped_public_structured_summary);
    let extra = extractor_name.map(|_| {
        let mut extra = wrapper
            .as_ref()
            .and_then(|value| value.get("extra"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        if let (Some(version), serde_json::Value::Object(map)) = (extractor_version, &mut extra) {
            map.insert("extractor_version".to_string(), version.into());
        }
        extra
    });
    (
        follow_crawl_urls,
        extra,
        structured,
        structured_for_embedding,
    )
}

pub fn extract_markdown_links(markdown: &str) -> Vec<serde_json::Value> {
    const LIMIT: usize = 512;
    let mut links = Vec::new();
    let bytes = markdown.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 3 < len && links.len() < LIMIT {
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            let href_start = i + 2;
            if let Some(rel) = bytes[href_start..].iter().position(|&b| b == b')') {
                let href = &markdown[href_start..href_start + rel];
                if href.starts_with("http://") || href.starts_with("https://") {
                    let text_end = i;
                    let text_start = markdown[..text_end]
                        .rfind('[')
                        .map(|position| position + 1)
                        .unwrap_or(text_end);
                    let text = &markdown[text_start..text_end];
                    links.push(serde_json::json!({ "href": href, "text": text }));
                }
                i = href_start + rel + 1;
                continue;
            }
        }
        i += 1;
    }
    links
}

fn capped_public_structured_summary(value: serde_json::Value) -> Option<serde_json::Value> {
    let bytes = serde_json::to_vec(&value).ok()?;
    if bytes.len() > MAX_PUBLIC_STRUCTURED_BYTES {
        None
    } else {
        Some(value)
    }
}

fn redact_sensitive_structured_keys(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .filter_map(|(key, value)| {
                    let sensitive = axon_core::redact::is_secret_like(&key)
                        || axon_core::redact::forbidden_field_name(&key);
                    (!sensitive).then(|| (key, redact_sensitive_structured_keys(value)))
                })
                .collect(),
        ),
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .into_iter()
                .map(redact_sensitive_structured_keys)
                .collect(),
        ),
        other => other,
    }
}

pub async fn validate_and_normalize_scrape_url(
    url: &str,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
) -> Result<String, Box<dyn Error>> {
    let normalized = normalize_url(url);
    emit(
        tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("scrape starting: {normalized}"),
        },
    )
    .await;
    tokio::time::timeout(
        Duration::from_millis(2000),
        axon_core::http::validate_url_with_dns(&normalized),
    )
    .await
    .map_err(|_| -> Box<dyn Error> {
        format!("invalid scrape url {normalized}: DNS validation timed out").into()
    })?
    .map_err(|e| -> Box<dyn Error> { format!("invalid scrape url {normalized}: {e}").into() })?;
    Ok(normalized.into_owned())
}

pub const MAX_SCRAPE_BATCH_URLS: usize = 50;

#[derive(Debug)]
enum ScrapeBatchError {
    Validation(String),
    Scrape(String),
}

impl fmt::Display for ScrapeBatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) | Self::Scrape(message) => f.write_str(message),
        }
    }
}

impl Error for ScrapeBatchError {}

/// Scrape a bounded batch of URLs. The cap lives in the service layer so CLI,
/// MCP, and REST callers share the same protection.
#[must_use = "scrape_batch returns a Result that should be handled"]
pub async fn scrape_batch(
    cfg: &Config,
    urls: &[String],
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<Vec<ScrapeResult>, Box<dyn Error>> {
    if urls.is_empty() {
        return Err("at least one url is required".into());
    }
    if urls.len() > MAX_SCRAPE_BATCH_URLS {
        return Err(
            format!("scrape accepts at most {MAX_SCRAPE_BATCH_URLS} urls per request").into(),
        );
    }
    let deadline = Duration::from_secs(cfg.scrape_batch_timeout_secs.max(1));
    run_with_scrape_batch_timeout(deadline, scrape_batch_inner(cfg, urls, tx)).await
}

async fn run_with_scrape_batch_timeout<F, T>(
    deadline: Duration,
    future: F,
) -> Result<T, Box<dyn Error>>
where
    F: Future<Output = Result<T, ScrapeBatchError>>,
{
    tokio::time::timeout(deadline, future)
        .await
        .map_err(|_| -> Box<dyn Error> {
            format!("scrape batch timed out after {}s", deadline.as_secs()).into()
        })?
        .map_err(|err| -> Box<dyn Error> { err.to_string().into() })
}

async fn scrape_batch_inner(
    cfg: &Config,
    urls: &[String],
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<Vec<ScrapeResult>, ScrapeBatchError> {
    let normalized: Vec<(usize, String)> = urls
        .iter()
        .enumerate()
        .map(|(idx, url)| (idx, normalize_url(url).into_owned()))
        .collect();
    let validated = stream::iter(normalized)
        .map(|(idx, url)| async move {
            tokio::time::timeout(
                Duration::from_millis(2000),
                axon_core::http::validate_url_with_dns(&url),
            )
            .await
            .map_err(|_| format!("invalid scrape url {url}: DNS validation timed out"))?
            .map_err(|e| format!("invalid scrape url {url}: {e}"))?;
            Ok::<(usize, String), String>((idx, url))
        })
        .buffer_unordered(10)
        .collect::<Vec<_>>()
        .await;

    let mut ready = Vec::with_capacity(validated.len());
    for item in validated {
        ready.push(item.map_err(ScrapeBatchError::Validation)?);
    }
    ready.sort_by_key(|(idx, _)| *idx);

    let scraped = stream::iter(ready)
        .map(|(idx, url)| {
            let tx = tx.clone();
            async move {
                scrape(cfg, &url, tx)
                    .await
                    .map(|result| (idx, result))
                    .map_err(|err| err.to_string())
            }
        })
        .buffer_unordered(10)
        .collect::<Vec<_>>()
        .await;

    let mut indexed_results = Vec::with_capacity(scraped.len());
    for item in scraped {
        indexed_results.push(item.map_err(ScrapeBatchError::Scrape)?);
    }
    indexed_results.sort_by_key(|(idx, _)| *idx);
    let results = indexed_results
        .into_iter()
        .map(|(_, result)| result)
        .collect();
    Ok(results)
}

#[cfg(test)]
#[path = "scrape_tests.rs"]
mod tests;
