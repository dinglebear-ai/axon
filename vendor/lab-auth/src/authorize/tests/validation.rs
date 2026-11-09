use super::*;

#[tokio::test]
async fn authorize_rejects_missing_or_invalid_response_type() {
    let app = router(test_auth_state_with_registered_client().await);
    for uri in [
        "/authorize?client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&scope=lab&code_challenge=pkce&code_challenge_method=S256",
        "/authorize?response_type=token&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&scope=lab&code_challenge=pkce&code_challenge_method=S256",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}

#[test]
fn validate_scope_accepts_configured_default_and_rejects_others() {
    // Empty scope falls back to configured default.
    assert_eq!(
        super::validate_scope(
            "",
            "syslog:read",
            &["syslog:read".to_string(), "syslog:admin".to_string()]
        )
        .unwrap(),
        "syslog:read"
    );
    // Matching scope passes through.
    assert_eq!(
        super::validate_scope(
            "syslog:read",
            "syslog:read",
            &["syslog:read".to_string(), "syslog:admin".to_string()]
        )
        .unwrap(),
        "syslog:read"
    );
    // Anything else is rejected — and the error mentions the configured
    // default (proving the LAB_SCOPE constant is gone).
    let err = super::validate_scope(
        "lab",
        "syslog:read",
        &["syslog:read".to_string(), "syslog:admin".to_string()],
    )
    .unwrap_err();
    assert!(err.to_string().contains("syslog:read"), "got: {err}");
}

#[tokio::test]
async fn authorize_rejects_invalid_scope() {
    let app = router(test_auth_state_with_registered_client().await);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/authorize?response_type=code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&scope=lab:admin&code_challenge=pkce&code_challenge_method=S256")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn authorize_rejects_mismatched_resource_parameter() {
    let app = router(test_auth_state_with_registered_client().await);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/authorize?response_type=code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&resource=https://other.example.com/mcp&scope=lab&code_challenge=pkce&code_challenge_method=S256")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn callback_rejects_expired_state() {
    let state = test_auth_state_with_registered_client().await;
    state
        .store
        .insert_authorization_request(AuthorizationRequestRow {
            state: "expired-state".to_string(),
            client_id: "client".to_string(),
            redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
            client_state: "client-state".to_string(),
            scope: "lab".to_string(),
            provider_code_verifier: "provider-verifier".to_string(),
            code_challenge: "challenge".to_string(),
            code_challenge_method: "S256".to_string(),
            created_at: now_unix() - 300,
            expires_at: now_unix() - 1,
        })
        .await
        .unwrap();
    let app = router(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/google/callback?state=expired-state&code=upstream-code")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
