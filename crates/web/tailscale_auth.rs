//! Token-based authentication for the web surfaces.
//!
//! Historical note: this module previously handled Tailscale and SSH auth too.
//! The current development model is intentionally simpler: one shared API token
//! (`AXON_WEB_API_TOKEN`) gates `/ws`, `/output/*`, and `/download/*`.

use axum::http::HeaderMap;
use subtle::ConstantTimeEq;

/// The result of checking auth on an incoming request.
#[derive(Debug, PartialEq, Eq)]
pub enum AuthOutcome {
    /// Authenticated via API token.
    Token,
    /// Not authenticated.
    Denied(DenyReason),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum DenyReason {
    /// No token was provided.
    NoCredentials,
    /// A token was provided but it did not match `AXON_WEB_API_TOKEN`.
    InvalidToken,
    /// No auth method is configured and this is a release build.
    NoAuthConfigured,
}

/// Constant-time byte comparison to prevent timing attacks on API token checks.
///
/// Uses `subtle::ConstantTimeEq` which handles length-mismatch without branching,
/// preventing token length leakage via response timing.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

/// Determine whether a request is authorized to access the axon web surfaces.
///
/// Token resolution order (first non-empty value wins):
/// 1. `Authorization: Bearer <token>` request header
/// 2. `x-api-key: <token>` request header
/// 3. `?token=` query parameter
pub fn check_auth(
    headers: &HeaderMap,
    query_token: Option<&str>,
    api_token: Option<&str>,
) -> AuthOutcome {
    // Extract token from request headers before falling back to the query
    // parameter. This keeps tokens out of access logs for callers that can
    // set custom headers (e.g. the Next.js proxy and API clients).
    //
    // RFC 6750 §2.1: the auth-scheme is case-insensitive, so we accept
    // "Bearer", "bearer", "BEARER", etc.
    let header_token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .and_then(|v| {
            let (scheme, token) = v.split_once(' ')?;
            scheme
                .eq_ignore_ascii_case("Bearer")
                .then_some(token.trim())
                .filter(|s| !s.is_empty())
        })
        .or_else(|| {
            headers
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty())
        });

    let provided_token = header_token.or(query_token);

    if let Some(expected) = api_token {
        let provided = provided_token.unwrap_or("").trim();
        if provided.is_empty() {
            return AuthOutcome::Denied(DenyReason::NoCredentials);
        }
        if constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
            return AuthOutcome::Token;
        }
        return AuthOutcome::Denied(DenyReason::InvalidToken);
    }

    #[cfg(any(debug_assertions, test))]
    {
        // Warn once per process, not on every check_auth() call.
        static WARNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        WARNED.get_or_init(|| {
            log::warn!(
                "[auth] AXON_WEB_API_TOKEN is not set — auth is DISABLED in this debug build"
            );
        });
        AuthOutcome::Token
    }
    #[cfg(not(any(debug_assertions, test)))]
    {
        AuthOutcome::Denied(DenyReason::NoAuthConfigured)
    }
}

/// Format a human-readable log message for an auth outcome.
pub fn auth_log_message(outcome: &AuthOutcome, addr: std::net::SocketAddr) -> String {
    match outcome {
        AuthOutcome::Token => format!("ws auth: api token from {}", addr.ip()),
        AuthOutcome::Denied(reason) => match reason {
            DenyReason::NoCredentials => {
                format!("ws denied: no credentials from {}", addr.ip())
            }
            DenyReason::InvalidToken => {
                format!("ws denied: invalid token from {}", addr.ip())
            }
            DenyReason::NoAuthConfigured => {
                format!(
                    "ws denied: no auth configured (set AXON_WEB_API_TOKEN) from {}",
                    addr.ip()
                )
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn constant_time_eq_equal_strings() {
        assert!(constant_time_eq(b"secret-token", b"secret-token"));
    }

    #[test]
    fn constant_time_eq_different_strings() {
        assert!(!constant_time_eq(b"secret-token", b"wrong-token-x"));
    }

    #[test]
    fn token_auth_succeeds_for_matching_token() {
        let outcome = check_auth(
            &HeaderMap::new(),
            Some("correct-token"),
            Some("correct-token"),
        );
        assert!(matches!(outcome, AuthOutcome::Token));
    }

    #[test]
    fn token_auth_rejects_wrong_token() {
        let outcome = check_auth(
            &HeaderMap::new(),
            Some("wrong-token"),
            Some("correct-token"),
        );
        assert!(matches!(
            outcome,
            AuthOutcome::Denied(DenyReason::InvalidToken)
        ));
    }

    #[test]
    fn token_auth_rejects_missing_token() {
        let outcome = check_auth(&HeaderMap::new(), None, Some("correct-token"));
        assert!(matches!(
            outcome,
            AuthOutcome::Denied(DenyReason::NoCredentials)
        ));
    }

    #[test]
    fn token_auth_succeeds_via_authorization_bearer_header() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer correct-token".parse().unwrap());
        let outcome = check_auth(&headers, None, Some("correct-token"));
        assert!(matches!(outcome, AuthOutcome::Token));
    }

    #[test]
    fn token_auth_succeeds_via_x_api_key_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "correct-token".parse().unwrap());
        let outcome = check_auth(&headers, None, Some("correct-token"));
        assert!(matches!(outcome, AuthOutcome::Token));
    }

    #[test]
    fn token_auth_header_takes_precedence_over_query_param() {
        // Header supplies the correct token; query param has a wrong one.
        // The header must win and produce Token, not InvalidToken.
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "correct-token".parse().unwrap());
        let outcome = check_auth(&headers, Some("wrong-token"), Some("correct-token"));
        assert!(matches!(outcome, AuthOutcome::Token));
    }

    #[test]
    fn token_auth_succeeds_for_lowercase_bearer_header() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "bearer correct-token".parse().unwrap());
        let outcome = check_auth(&headers, None, Some("correct-token"));
        assert!(matches!(outcome, AuthOutcome::Token));
    }

    #[test]
    fn token_auth_rejects_wrong_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer wrong-token".parse().unwrap());
        let outcome = check_auth(&headers, None, Some("correct-token"));
        assert!(matches!(
            outcome,
            AuthOutcome::Denied(DenyReason::InvalidToken)
        ));
    }
}
