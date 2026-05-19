use spider::url::Url;

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
