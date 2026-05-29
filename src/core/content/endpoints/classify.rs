use crate::services::types::EndpointKind;

pub(super) fn looks_like_endpoint(value: &str) -> bool {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    lower.starts_with("ws://")
        || lower.starts_with("wss://")
        || lower.contains("graphql")
        || lower.contains("/gql")
        || lower.contains("/api")
        || lower.contains("/v1/")
        || lower.contains("/v2/")
        || lower.contains("/rest")
        || lower.contains("/gateway")
}

pub(super) fn classify_value(value: &str) -> EndpointKind {
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("ws://") || lower.starts_with("wss://") {
        EndpointKind::Websocket
    } else if lower.contains("graphql") || lower.contains("/gql") {
        EndpointKind::Graphql
    } else if lower.starts_with("http://") || lower.starts_with("https://") {
        EndpointKind::AbsoluteUrl
    } else {
        EndpointKind::RelativePath
    }
}

pub(super) fn classify_relative(value: &str) -> EndpointKind {
    let lower = value.to_ascii_lowercase();
    if lower.contains("graphql") || lower.contains("/gql") {
        EndpointKind::Graphql
    } else {
        EndpointKind::RelativePath
    }
}

pub(super) fn classify_absolute(value: &str) -> EndpointKind {
    let lower = value.to_ascii_lowercase();
    if lower.contains("graphql") || lower.contains("/gql") {
        EndpointKind::Graphql
    } else {
        EndpointKind::AbsoluteUrl
    }
}

pub(super) fn is_noise_value(value: &str) -> bool {
    if value.len() < 4 {
        return true;
    }
    let lower = value.to_ascii_lowercase();
    if matches!(lower.as_str(), "/api" | "/api/" | "/rest" | "/rest/") {
        return true;
    }
    // Noise namespace/spec hosts that flood results with non-endpoint strings.
    if lower.contains("schema.org")
        || lower.contains("json-schema.org")
        || lower.contains("w3.org")
        || lower.contains("example.com")
        || lower.contains("example.org")
        || lower.contains("example.net")
    {
        return true;
    }
    // Static assets are not API endpoints.
    if let Some(path) = lower.split('?').next() {
        return path.ends_with(".js")
            || path.ends_with(".css")
            || path.ends_with(".png")
            || path.ends_with(".jpg")
            || path.ends_with(".jpeg")
            || path.ends_with(".gif")
            || path.ends_with(".svg")
            || path.ends_with(".ico")
            || path.ends_with(".woff")
            || path.ends_with(".woff2")
            || path.ends_with(".ttf")
            || path.ends_with(".eot")
            || path.ends_with(".otf")
            || path.ends_with(".webp")
            || path.ends_with(".avif")
            || path.ends_with(".mp4")
            || path.ends_with(".webm")
            || path.ends_with(".mp3")
            || path.ends_with(".pdf")
            || path.ends_with(".map");
    }
    false
}

/// Returns false for obvious minifier garbage: single-label domains, single-char TLDs,
/// and hosts that are too short to be real (e.g. `http://n/path`, `http://f`).
pub(super) fn is_valid_absolute_host(value: &str) -> bool {
    let host = if let Some(rest) = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .or_else(|| value.strip_prefix("ws://"))
        .or_else(|| value.strip_prefix("wss://"))
    {
        // Stop at path, port, or query
        rest.split(['/', ':', '?', '#']).next().unwrap_or("")
    } else {
        return true; // not an absolute URL — caller decides
    };
    if host.is_empty() {
        return false;
    }
    // Must have at least two labels (e.g. "foo.com", not just "foo")
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    // TLD must be at least 2 alphabetic chars
    let tld = labels.last().unwrap_or(&"");
    if tld.len() < 2 || !tld.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    // Each label must be non-empty and at least 1 char
    labels.iter().all(|label| !label.is_empty())
}
