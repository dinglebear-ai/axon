use super::*;

pub(super) fn sanitize_return_to(state: &AuthState, requested: Option<&str>) -> String {
    let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) else {
        return "/".to_string();
    };
    if requested.starts_with('/') && !requested.starts_with("//") {
        return requested.to_string();
    }
    let Some(public_url) = state.config.public_url.as_ref() else {
        return "/".to_string();
    };
    let Ok(url) = reqwest::Url::parse(requested) else {
        return "/".to_string();
    };
    if url.scheme() != public_url.scheme()
        || url.host_str() != public_url.host_str()
        || url.port_or_known_default() != public_url.port_or_known_default()
    {
        return "/".to_string();
    }
    let mut normalized = url.path().to_string();
    if let Some(query) = url.query() {
        normalized.push('?');
        normalized.push_str(query);
    }
    if let Some(fragment) = url.fragment() {
        normalized.push('#');
        normalized.push_str(fragment);
    }
    normalized
}

pub(super) fn validate_response_type(response_type: &str) -> Result<(), AuthError> {
    if response_type == "code" {
        Ok(())
    } else {
        warn!(
            response_type = %response_type,
            "oauth authorize rejected: unsupported response_type"
        );
        Err(AuthError::Validation(
            "response_type must be `code`".to_string(),
        ))
    }
}

pub(super) fn validate_scope(
    scope: &str,
    default_scope: &str,
    scopes_supported: &[String],
) -> Result<String, AuthError> {
    let normalized = scope.trim();
    if normalized.is_empty() {
        return Ok(default_scope.to_string());
    }
    // Accept any space-separated combination where every token is in scopes_supported.
    // Falls back to exact-match against default_scope when scopes_supported is empty.
    if !scopes_supported.is_empty() {
        let all_valid = normalized
            .split_whitespace()
            .all(|s| scopes_supported.iter().any(|sup| sup == s));
        if all_valid {
            return Ok(normalized.to_string());
        }
    } else if normalized == default_scope {
        return Ok(normalized.to_string());
    }
    warn!(scope = %normalized, "oauth authorize rejected: unsupported scope");
    Err(AuthError::Validation(format!(
        "scope must be one of: {}",
        scopes_supported.join(", ")
    )))
}

pub(crate) fn validate_resource(
    state: &AuthState,
    requested: Option<&str>,
) -> Result<(), AuthError> {
    let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let expected = crate::metadata::canonical_resource_url(state);
    if requested == expected {
        return Ok(());
    }

    warn!(
        requested_resource = %requested,
        expected_resource = %expected,
        "oauth request rejected: resource does not match canonical MCP endpoint"
    );
    Err(AuthError::Validation(format!(
        "resource must be `{expected}`"
    )))
}

pub(super) fn is_loopback_redirect(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    if url.scheme() != "http" {
        return false;
    }
    matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))
}

pub(super) fn is_allowed_redirect_uri(value: &str, patterns: &[String]) -> bool {
    if is_loopback_redirect(value) {
        return true;
    }

    let Ok(candidate) = reqwest::Url::parse(value) else {
        return false;
    };
    patterns
        .iter()
        .any(|pattern| redirect_pattern_matches(pattern, &candidate))
}

pub(super) fn wildcard_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == value;
    }

    let anchored_start = !pattern.starts_with('*');
    let anchored_end = !pattern.ends_with('*');
    let non_empty_parts: Vec<&str> = parts.into_iter().filter(|part| !part.is_empty()).collect();
    if non_empty_parts.is_empty() {
        return true;
    }

    let mut cursor = 0usize;
    for (index, part) in non_empty_parts.iter().enumerate() {
        if index == 0 && anchored_start {
            if !value[cursor..].starts_with(part) {
                return false;
            }
            cursor += part.len();
            continue;
        }

        match value[cursor..].find(part) {
            Some(found) => cursor += found + part.len(),
            None => return false,
        }
    }

    if anchored_end && let Some(last) = non_empty_parts.last() {
        return value.ends_with(last);
    }

    true
}

pub(super) fn redirect_pattern_matches(pattern: &str, candidate: &reqwest::Url) -> bool {
    let Ok(pattern) = reqwest::Url::parse(pattern) else {
        return false;
    };
    if pattern.scheme() != candidate.scheme() {
        return false;
    }
    if pattern.port_or_known_default() != candidate.port_or_known_default() {
        return false;
    }
    let Some(pattern_host) = pattern.host_str() else {
        return false;
    };
    let Some(candidate_host) = candidate.host_str() else {
        return false;
    };
    if !host_pattern_matches(pattern_host, candidate_host) {
        return false;
    }
    if !wildcard_matches(pattern.path(), candidate.path()) {
        return false;
    }

    match (pattern.query(), candidate.query()) {
        (Some(pattern_query), Some(candidate_query)) => {
            wildcard_matches(pattern_query, candidate_query)
        }
        (None, None) => true,
        _ => false,
    }
}

pub(super) fn host_pattern_matches(pattern_host: &str, candidate_host: &str) -> bool {
    let pattern_labels = pattern_host.split('.').collect::<Vec<_>>();
    let candidate_labels = candidate_host.split('.').collect::<Vec<_>>();
    if pattern_labels.len() != candidate_labels.len() {
        return false;
    }

    pattern_labels
        .iter()
        .zip(candidate_labels.iter())
        .all(|(pattern, candidate)| {
            *pattern == "*" || (!pattern.contains('*') && pattern.eq_ignore_ascii_case(candidate))
        })
}
