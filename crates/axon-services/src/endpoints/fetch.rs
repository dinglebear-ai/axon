use super::{
    BUNDLE_FETCH_SEMAPHORE, EndpointError, MAX_BUNDLE_BYTES, validate_url_with_dns_timeout,
};
use axon_core::content::PrefetchedBundle;
use futures_util::{StreamExt, stream};

pub(super) async fn fetch_bundles(
    client: &reqwest::Client,
    sources: &[axon_core::content::ScriptSource],
    max_scan_bytes: usize,
) -> Vec<Result<PrefetchedBundle, String>> {
    stream::iter(sources.iter().cloned())
        .map(|source| async move {
            let _permit = BUNDLE_FETCH_SEMAPHORE
                .acquire()
                .await
                .map_err(|err| format!("bundle fetch semaphore closed: {err}"))?;
            fetch_script_bundle(client, &source.url, max_scan_bytes)
                .await
                .map_err(|err| format!("bundle fetch skipped {}: {err}", source.url))
        })
        .buffer_unordered(8)
        .collect()
        .await
}

async fn fetch_script_bundle(
    client: &reqwest::Client,
    url: &str,
    max_scan_bytes: usize,
) -> Result<PrefetchedBundle, EndpointError> {
    validate_url_with_dns_timeout(url).await?;
    let response = client.get(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}").into());
    }
    if let Some(content_type) = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    {
        let lower = content_type.to_ascii_lowercase();
        if !(lower.contains("javascript")
            || lower.contains("ecmascript")
            || lower.contains("text/plain")
            || lower.contains("application/octet-stream"))
        {
            return Err(format!("content-type {content_type} is not JavaScript-like").into());
        }
    }
    fetch_response_text(response, max_scan_bytes.min(MAX_BUNDLE_BYTES))
        .await
        .map(|(text, truncated)| PrefetchedBundle {
            url: url.to_string(),
            text,
            truncated,
        })
}

pub(super) async fn fetch_bounded_text(
    client: &reqwest::Client,
    url: &str,
    max_bytes: usize,
    require_success: bool,
) -> Result<(String, bool), EndpointError> {
    let response = client.get(url).send().await?;
    if require_success {
        response.error_for_status_ref()?;
    }
    fetch_response_text(response, max_bytes).await
}

async fn fetch_response_text(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<(String, bool), EndpointError> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            let remaining = max_bytes.saturating_sub(bytes.len());
            bytes.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}
