use crate::services::types::EndpointKind;

pub(super) fn looks_like_endpoint(value: &str) -> bool {
    let value = value.trim();
    value.starts_with("ws://")
        || value.starts_with("wss://")
        || value.contains("graphql")
        || value.contains("/gql")
        || value.contains("/api")
        || value.contains("/v1/")
        || value.contains("/v2/")
        || value.contains("/rest")
        || value.contains("/gateway")
}

pub(super) fn classify_value(value: &str) -> EndpointKind {
    if value.starts_with("ws://") || value.starts_with("wss://") {
        EndpointKind::Websocket
    } else if value.to_ascii_lowercase().contains("graphql") || value.contains("/gql") {
        EndpointKind::Graphql
    } else if value.starts_with("http://") || value.starts_with("https://") {
        EndpointKind::AbsoluteUrl
    } else {
        EndpointKind::RelativePath
    }
}

pub(super) fn classify_relative(value: &str) -> EndpointKind {
    if value.to_ascii_lowercase().contains("graphql") || value.contains("/gql") {
        EndpointKind::Graphql
    } else {
        EndpointKind::RelativePath
    }
}

pub(super) fn classify_absolute(value: &str) -> EndpointKind {
    if value.to_ascii_lowercase().contains("graphql") || value.contains("/gql") {
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
