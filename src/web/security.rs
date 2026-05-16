use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct HostAllowlist {
    allowed: Arc<Vec<String>>,
}

impl HostAllowlist {
    pub fn new(bind_host: &str, port: u16, allowed_origins: &[String]) -> Self {
        let mut allowed = vec![
            format!("127.0.0.1:{port}"),
            format!("localhost:{port}"),
            format!("[::1]:{port}"),
        ];

        let trimmed = bind_host.trim();
        if !trimmed.is_empty() {
            allowed.push(format!("{}:{port}", trimmed.trim_matches(['[', ']'])));
            if trimmed.contains(':') && !trimmed.starts_with('[') {
                allowed.push(format!("[{trimmed}]:{port}"));
            }
        }

        for origin in allowed_origins {
            if let Some(authority) = parse_origin_authority(origin) {
                allowed.push(authority);
            }
        }
        allowed.sort();
        allowed.dedup();
        Self {
            allowed: Arc::new(allowed),
        }
    }

    fn allows(&self, host: &str) -> bool {
        let host = host.trim();
        self.allowed
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host))
    }
}

pub(crate) async fn host_validation_middleware(
    State(allowlist): State<HostAllowlist>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let Some(host) = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
    else {
        return (StatusCode::BAD_REQUEST, "bad request: missing Host header").into_response();
    };

    if !allowlist.allows(host) {
        tracing::warn!(host = %host, "web: rejected request with disallowed Host header");
        return (StatusCode::FORBIDDEN, "forbidden: host not allowed").into_response();
    }

    next.run(request).await
}

fn parse_origin_authority(origin: &str) -> Option<String> {
    origin
        .parse::<axum::http::Uri>()
        .ok()
        .and_then(|uri| uri.authority().map(|authority| authority.to_string()))
}

#[cfg(test)]
#[path = "security_tests.rs"]
mod tests;
