//! CORS middleware and origin-checking helpers for the axon web server.

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use crate::crates::core::config::Config;

const DEFAULT_CORS_ALLOW_HEADERS: &str = "authorization, content-type, x-api-key";
const DEFAULT_CORS_ALLOW_METHODS: &str = "GET, POST, OPTIONS";
const CORS_VARY_VALUE: &str =
    "Origin, Access-Control-Request-Method, Access-Control-Request-Headers";

pub(crate) async fn web_cors_middleware(
    axum::extract::State(cfg): axum::extract::State<Arc<Config>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    cors_middleware(request, next, &cfg.web_allowed_origins).await
}

pub(crate) async fn cors_middleware(
    request: Request<Body>,
    next: Next,
    allowed_origins: &[String],
) -> Response {
    let origin = request.headers().get(header::ORIGIN).cloned();
    let host = request.headers().get(header::HOST).cloned();
    let allow_origin = origin
        .as_ref()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            cors_origin_header_value(
                value,
                host.as_ref().and_then(|header| header.to_str().ok()),
                allowed_origins,
            )
        });

    if request.method() == Method::OPTIONS && origin.is_some() {
        return match allow_origin {
            Some(allow_origin) => preflight_cors_response(&request, allow_origin),
            None => (StatusCode::FORBIDDEN, "forbidden: origin not allowed").into_response(),
        };
    }

    if origin.is_some() && allow_origin.is_none() {
        return (StatusCode::FORBIDDEN, "forbidden: origin not allowed").into_response();
    }

    let mut response = next.run(request).await;
    if let Some(allow_origin) = allow_origin {
        set_cors_response_headers(response.headers_mut(), allow_origin);
    }
    response
}

fn preflight_cors_response(_request: &Request<Body>, allow_origin: HeaderValue) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NO_CONTENT;
    set_cors_response_headers(response.headers_mut(), allow_origin);

    // Always respond with a static explicit allowlist — never reflect the client-supplied
    // Access-Control-Request-Headers value, which would grant an effective wildcard for
    // any allowed origin (CWE-942).
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(DEFAULT_CORS_ALLOW_HEADERS),
    );
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static(DEFAULT_CORS_ALLOW_METHODS),
    );
    response.headers_mut().insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("600"),
    );
    response
}

fn set_cors_response_headers(headers: &mut HeaderMap, allow_origin: HeaderValue) {
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, allow_origin);
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    headers.insert(header::VARY, HeaderValue::from_static(CORS_VARY_VALUE));
}

pub(crate) fn effective_shell_allowed_origins<'a>(
    shell_allowed_origins: &'a [String],
    web_allowed_origins: &'a [String],
) -> &'a [String] {
    if shell_allowed_origins.is_empty() {
        web_allowed_origins
    } else {
        shell_allowed_origins
    }
}

pub(super) fn websocket_origin_is_allowed(headers: &HeaderMap, allowed_origins: &[String]) -> bool {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return true;
    };
    cors_origin_header_value(
        origin,
        headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok()),
        allowed_origins,
    )
    .is_some()
}

pub(crate) fn cors_origin_header_value(
    origin: &str,
    request_host: Option<&str>,
    allowed_origins: &[String],
) -> Option<HeaderValue> {
    let is_allowed = if allowed_origins.is_empty() {
        origin_matches_host(origin, request_host?)
    } else {
        allowed_origins.iter().any(|allowed| allowed == origin)
    };

    is_allowed
        .then(|| HeaderValue::from_str(origin).ok())
        .flatten()
}

fn origin_matches_host(origin: &str, request_host: &str) -> bool {
    parse_origin_authority(origin)
        .map(|origin_host| origin_host.eq_ignore_ascii_case(request_host.trim()))
        .unwrap_or(false)
}

fn parse_origin_authority(origin: &str) -> Option<String> {
    origin
        .parse::<Uri>()
        .ok()
        .and_then(|uri| uri.authority().map(|authority| authority.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_allows_explicit_origin() {
        let allowed = vec!["https://axon.example.com".to_string()];
        let value = cors_origin_header_value(
            "https://axon.example.com",
            Some("127.0.0.1:49000"),
            &allowed,
        );

        assert_eq!(
            value.as_ref().and_then(|header| header.to_str().ok()),
            Some("https://axon.example.com")
        );
    }

    #[test]
    fn cors_allows_same_host_when_allowlist_is_empty() {
        let value =
            cors_origin_header_value("http://localhost:49000", Some("localhost:49000"), &[]);

        assert_eq!(
            value.as_ref().and_then(|header| header.to_str().ok()),
            Some("http://localhost:49000")
        );
    }

    #[test]
    fn cors_rejects_cross_origin_when_allowlist_is_empty() {
        let value =
            cors_origin_header_value("https://axon.example.com", Some("localhost:49000"), &[]);

        assert!(value.is_none());
    }

    #[test]
    fn shell_origin_allowlist_falls_back_to_web_allowlist() {
        let web_allowed = vec!["https://axon.example.com".to_string()];
        let shell_allowed: Vec<String> = Vec::new();

        assert_eq!(
            effective_shell_allowed_origins(&shell_allowed, &web_allowed),
            web_allowed.as_slice()
        );
    }
}
