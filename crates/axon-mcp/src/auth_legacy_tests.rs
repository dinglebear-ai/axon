use super::*;

pub(super) fn authorize_mcp_http_request_with_token(
    request: &Request<Body>,
    configured_token: Option<&str>,
) -> Result<(), StatusCode> {
    if configured_token
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        static WARNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        WARNED.get_or_init(|| {
            tracing::warn!(
                context = "mcp_auth",
                "AXON_HTTP_TOKEN not set \u{2014} MCP HTTP server is unauthenticated"
            );
        });
    }
    authorize_mcp_http_headers(request.headers(), configured_token)
}
