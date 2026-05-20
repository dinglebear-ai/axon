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
    matches!(lower.as_str(), "/api" | "/api/" | "/rest" | "/rest/")
        || lower.contains("schema.org")
        || lower.contains("json-schema.org")
}
