use super::*;

#[tokio::test]
async fn register_accepts_public_dcr_and_enforces_loopback_redirects() {
    let mut config = test_auth_config();
    config.enable_dynamic_registration = true;
    let app = router(test_auth_state_with_config(config).await);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "redirect_uris": ["http://127.0.0.1:7777/callback"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let rejected = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "redirect_uris": ["https://claude.ai/api/mcp/auth_callback"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn register_accepts_allowed_non_loopback_redirect_patterns() {
    let mut config = test_auth_config();
    config.enable_dynamic_registration = true;
    config.allowed_client_redirect_uris = vec!["https://callback.tootie.tv/callback/*".to_string()];
    let app = router(test_auth_state_with_config(config).await);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "redirect_uris": ["https://callback.tootie.tv/callback/dookie"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn register_accepts_native_callback_without_redirect_allowlist() {
    let mut config = test_auth_config();
    config.enable_dynamic_registration = true;
    config.allowed_client_redirect_uris.clear();
    let app = router(test_auth_state_with_config(config).await);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "redirect_uris": ["https://lab.example.com/native/callback"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn register_is_rate_limited_after_configured_burst() {
    let mut config = test_auth_config();
    config.enable_dynamic_registration = true;
    config.register_requests_per_minute = 1;
    let app = router(test_auth_state_with_config(config).await);

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "redirect_uris": ["http://127.0.0.1:7777/callback"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let second = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "redirect_uris": ["http://127.0.0.1:8888/callback"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn native_callback_does_not_accept_public_code_writes() {
    let app = router(test_auth_state().await);
    let callback = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/native/callback?code=code-123&state=state-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(callback.status(), StatusCode::GONE);

    let poll = app
        .oneshot(
            Request::builder()
                .uri("/native/poll?state=state-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(poll.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn oauth_callback_stashes_native_code_for_one_poll() {
    let state = test_auth_state_with_mock_google().await;
    state
        .store
        .insert_authorization_request(AuthorizationRequestRow {
            state: "native-good-state".to_string(),
            client_id: "client".to_string(),
            redirect_uri: "https://lab.example.com/native/callback".to_string(),
            client_state: "state-123".to_string(),
            scope: "lab".to_string(),
            provider_code_verifier: "provider-verifier".to_string(),
            code_challenge: "challenge".to_string(),
            code_challenge_method: "S256".to_string(),
            created_at: now_unix(),
            expires_at: now_unix() + 300,
        })
        .await
        .unwrap();
    let app = router(state);
    let callback = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/google/callback?state=native-good-state&code=upstream-code")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(callback.status(), StatusCode::OK);

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/native/poll?state=state-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let body = axum::body::to_bytes(first.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["code"].as_str().is_some_and(|code| !code.is_empty()));

    let second = app
        .oneshot(
            Request::builder()
                .uri("/native/poll?state=state-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::ACCEPTED);
}
