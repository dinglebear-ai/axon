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
mod tests {
    use super::endpoint_snapshot;

    #[test]
    fn endpoint_snapshot_falls_back_for_process_local_urls() {
        let mut fallback_fields = Vec::new();

        let snapshot = endpoint_snapshot("tei_url", "http://localhost:80", &mut fallback_fields)
            .expect("valid local endpoint");

        assert_eq!(snapshot, None);
        assert_eq!(fallback_fields, vec!["tei_url".to_string()]);
    }

    #[test]
    fn endpoint_snapshot_rejects_malformed_endpoint_urls() {
        let mut fallback_fields = Vec::new();

        let err = endpoint_snapshot("tei_url", "not a url", &mut fallback_fields)
            .expect_err("malformed endpoint must fail");

        assert!(err.contains("invalid tei_url"), "unexpected error: {err}");
        assert!(fallback_fields.is_empty());
    }
}
