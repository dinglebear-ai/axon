use super::*;

fn meta() -> AuthServerMetadata {
    AuthServerMetadata {
        authorization_endpoint: "https://axon.example.com/authorize".to_string(),
        token_endpoint: "https://axon.example.com/token".to_string(),
        registration_endpoint: Some("https://axon.example.com/register".to_string()),
        native_callback_endpoint: Some("https://axon.example.com/native/callback".to_string()),
        native_poll_endpoint: Some("https://axon.example.com/native/poll".to_string()),
    }
}

#[test]
fn discovery_url_appends_well_known_path() {
    assert_eq!(
        discovery_url("https://axon.example.com/"),
        "https://axon.example.com/.well-known/oauth-authorization-server"
    );
}

#[test]
fn metadata_deserializes_ignoring_extra_fields_and_optional_registration() {
    let json = r#"{
        "issuer": "https://axon.example.com",
        "authorization_endpoint": "https://axon.example.com/authorize",
        "token_endpoint": "https://axon.example.com/token",
        "registration_endpoint": "https://axon.example.com/register",
        "jwks_uri": "https://axon.example.com/jwks",
        "response_types_supported": ["code"]
    }"#;
    let parsed: AuthServerMetadata = serde_json::from_str(json).unwrap();
    assert_eq!(parsed.token_endpoint, "https://axon.example.com/token");
    assert_eq!(
        parsed.registration_endpoint.as_deref(),
        Some("https://axon.example.com/register")
    );
    assert!(parsed.native_callback_endpoint.is_none());
    assert!(parsed.native_poll_endpoint.is_none());

    let native = r#"{
        "issuer": "https://axon.example.com",
        "authorization_endpoint": "https://axon.example.com/authorize",
        "token_endpoint": "https://axon.example.com/token",
        "registration_endpoint": "https://axon.example.com/register",
        "native_callback_endpoint": "https://axon.example.com/native/callback",
        "native_poll_endpoint": "https://axon.example.com/native/poll"
    }"#;
    let parsed: AuthServerMetadata = serde_json::from_str(native).unwrap();
    assert_eq!(
        parsed.native_callback_endpoint.as_deref(),
        Some("https://axon.example.com/native/callback")
    );
    assert_eq!(
        parsed.native_poll_endpoint.as_deref(),
        Some("https://axon.example.com/native/poll")
    );

    // DCR-disabled server omits registration_endpoint → None, not a parse error.
    let no_dcr = r#"{
        "issuer": "https://axon.example.com",
        "authorization_endpoint": "https://axon.example.com/authorize",
        "token_endpoint": "https://axon.example.com/token"
    }"#;
    let parsed: AuthServerMetadata = serde_json::from_str(no_dcr).unwrap();
    assert!(parsed.registration_endpoint.is_none());
}

#[test]
fn token_response_deserializes_with_and_without_refresh() {
    let with = r#"{"access_token":"a","token_type":"Bearer","expires_in":3600,"refresh_token":"r","scope":"axon:read axon:write"}"#;
    let parsed: TokenResponse = serde_json::from_str(with).unwrap();
    assert_eq!(parsed.refresh_token.as_ref().map(|s| s.expose()), Some("r"));
    assert_eq!(parsed.expires_in, 3600);

    let without = r#"{"access_token":"a","token_type":"Bearer","expires_in":3600,"scope":"axon:read axon:write"}"#;
    let parsed: TokenResponse = serde_json::from_str(without).unwrap();
    assert!(parsed.refresh_token.is_none());
}

#[test]
fn token_response_debug_redacts_tokens() {
    let parsed: TokenResponse = serde_json::from_str(
        r#"{"access_token":"secret-a","token_type":"Bearer","expires_in":3600,"refresh_token":"secret-r","scope":"axon:read"}"#,
    )
    .unwrap();
    let rendered = format!("{parsed:?}");
    assert!(!rendered.contains("secret-a"));
    assert!(!rendered.contains("secret-r"));
}

#[test]
fn require_secure_url_allows_https_and_loopback_http_only() {
    assert!(require_secure_url("https://axon.example.com/token").is_ok());
    assert!(require_secure_url("http://127.0.0.1:8001/token").is_ok());
    assert!(require_secure_url("http://[::1]:8001/token").is_ok());
    assert!(require_secure_url("http://localhost:8001/token").is_ok());
    assert!(require_secure_url("http://axon.example.com/token").is_err()); // cleartext non-loopback
    assert!(require_secure_url("file:///etc/passwd").is_err());
    assert!(require_secure_url("not a url").is_err());
}

#[test]
fn authorize_url_carries_all_required_pkce_params() {
    let url = build_authorize_url(
        &meta(),
        "client-123",
        "http://127.0.0.1:7777/callback",
        "axon:read axon:write",
        "state-xyz",
        "challenge-abc",
    )
    .unwrap();
    assert!(url.starts_with("https://axon.example.com/authorize?"));
    assert!(url.contains("response_type=code"));
    assert!(url.contains("client_id=client-123"));
    assert!(url.contains("code_challenge=challenge-abc"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("state=state-xyz"));
    assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A7777%2Fcallback"));
    assert!(url.contains("scope=axon%3Aread+axon%3Awrite"));
}

#[test]
fn registration_body_wraps_single_redirect_uri() {
    assert_eq!(
        registration_body("http://127.0.0.1:7777/callback"),
        serde_json::json!({ "redirect_uris": ["http://127.0.0.1:7777/callback"] })
    );
}

#[test]
fn token_forms_have_required_fields() {
    let auth = authorization_code_form(
        "code-1",
        "client-123",
        "http://127.0.0.1:7777/callback",
        "verifier-1",
    );
    assert!(auth.contains(&("grant_type", "authorization_code".to_string())));
    assert!(auth.contains(&("code", "code-1".to_string())));
    assert!(auth.contains(&("client_id", "client-123".to_string())));
    assert!(auth.contains(&("redirect_uri", "http://127.0.0.1:7777/callback".to_string())));
    assert!(auth.contains(&("code_verifier", "verifier-1".to_string())));

    let refresh = refresh_form("client-123", "refresh-1");
    assert!(refresh.contains(&("grant_type", "refresh_token".to_string())));
    assert!(refresh.contains(&("refresh_token", "refresh-1".to_string())));
    assert!(refresh.contains(&("client_id", "client-123".to_string())));
}

#[test]
fn grant_rejection_only_for_definitive_codes_not_transient_4xx() {
    use reqwest::StatusCode;
    // Definitive grant rejections → clear the session.
    assert!(is_grant_rejection(StatusCode::BAD_REQUEST));
    assert!(is_grant_rejection(StatusCode::UNAUTHORIZED));
    assert!(is_grant_rejection(StatusCode::FORBIDDEN));
    assert!(is_grant_rejection(StatusCode::GONE));
    // Transient — must NOT wipe a valid OAuth session.
    assert!(!is_grant_rejection(StatusCode::TOO_MANY_REQUESTS)); // 429
    assert!(!is_grant_rejection(StatusCode::REQUEST_TIMEOUT)); // 408
    assert!(!is_grant_rejection(StatusCode::INTERNAL_SERVER_ERROR)); // 500
    assert!(!is_grant_rejection(StatusCode::SERVICE_UNAVAILABLE)); // 503
}

#[tokio::test]
async fn poll_native_code_times_out_stalled_response_within_budget() {
    let listener = tokio::net::TcpListener::bind(("localhost", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let _server = tokio::spawn(async move {
        let Ok((socket, _)) = listener.accept().await else {
            return;
        };
        tokio::time::sleep(Duration::from_secs(5)).await;
        drop(socket);
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    let start = tokio::time::Instant::now();
    let err = poll_native_code(
        &client,
        &format!("http://localhost:{port}/native/poll"),
        "state",
        Duration::from_millis(100),
    )
    .await
    .unwrap_err();

    assert!(err.contains("timed out"), "unexpected error: {err}");
    assert!(start.elapsed() < Duration::from_secs(2));
}
