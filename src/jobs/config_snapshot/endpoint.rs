use spider::url::Url;

use crate::core::config::Config;

pub(super) struct EndpointSnapshots {
    pub(super) tei_url: Option<String>,
    pub(super) qdrant_url: Option<String>,
    pub(super) openai_base_url: Option<String>,
}

pub(super) fn snapshot_endpoints(
    cfg: &Config,
    process_fallback_fields: &mut Vec<String>,
) -> Result<EndpointSnapshots, String> {
    Ok(EndpointSnapshots {
        tei_url: endpoint_snapshot("tei_url", &cfg.tei_url, process_fallback_fields)?,
        qdrant_url: endpoint_snapshot("qdrant_url", &cfg.qdrant_url, process_fallback_fields)?,
        openai_base_url: endpoint_snapshot(
            "openai_base_url",
            &cfg.openai_base_url,
            process_fallback_fields,
        )?,
    })
}

pub(super) fn snapshot_chrome_remote_url(
    cfg: &Config,
    process_fallback_fields: &mut Vec<String>,
) -> Result<Option<String>, String> {
    match cfg.chrome_remote_url.as_deref() {
        Some(url) => endpoint_snapshot("chrome_remote_url", url, process_fallback_fields),
        None => Ok(None),
    }
}

pub(super) fn endpoint_snapshot(
    name: &str,
    url: &str,
    process_fallback_fields: &mut Vec<String>,
) -> Result<Option<String>, String> {
    if endpoint_url_is_public(name, url)? {
        Ok(Some(url.to_string()))
    } else {
        process_fallback_fields.push(name.to_string());
        Ok(None)
    }
}

fn endpoint_url_is_public(name: &str, url: &str) -> Result<bool, String> {
    if url.trim().is_empty() {
        return Ok(true);
    }
    let parsed =
        Url::parse(url).map_err(|error| format!("invalid {name} in job config: {error}"))?;
    Ok(parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none()
        && !endpoint_host_is_process_local(&parsed))
}

fn endpoint_host_is_process_local(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host
        .trim_end_matches('.')
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    if host == "localhost" {
        return true;
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return ip.is_loopback() || ip.is_unspecified();
    }
    false
}

#[cfg(test)]
#[path = "endpoint_tests.rs"]
mod tests;
