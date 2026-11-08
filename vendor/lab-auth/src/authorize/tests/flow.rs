use super::*;

#[test]
fn wildcard_redirect_patterns_support_leading_and_infix_matches() {
    assert!(wildcard_matches(
        "https://callback.tootie.tv/callback/*",
        "https://callback.tootie.tv/callback/dookie"
    ));
    assert!(wildcard_matches(
        "https://callback.*.tv/callback/*",
        "https://callback.tootie.tv/callback/dookie"
    ));
    assert!(!wildcard_matches("/callback", "/callback/extra"));
}

#[test]
fn host_patterns_support_full_label_wildcards_only() {
    assert!(host_pattern_matches("callback.*.tv", "callback.tootie.tv"));
    assert!(host_pattern_matches(
        "*.example.com",
        "callback.example.com"
    ));
    assert!(!host_pattern_matches(
        "callback.example.com*",
        "callback.example.com"
    ));
    assert!(!host_pattern_matches(
        "*.example.com",
        "callback.nested.example.com"
    ));
}

#[test]
fn wildcard_redirect_patterns_do_not_overmatch_similar_hosts() {
    assert!(!is_allowed_redirect_uri(
        "https://callback.tootie.tv.evil.example/callback/dookie",
        &[String::from("https://callback.tootie.tv/callback/*")]
    ));
    assert!(!is_allowed_redirect_uri(
        "https://callback.example.com.evil.example/callback",
        &[String::from("https://callback.example.com*")]
    ));
}

#[tokio::test]
async fn authorize_persists_full_state_and_redirects_to_google() {
    let app = router(test_auth_state_with_registered_client().await);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/authorize?response_type=code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&scope=lab&code_challenge=pkce&code_challenge_method=S256")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FOUND);
    let location = response
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.contains("accounts.google.com"));
}

#[tokio::test]
async fn authorize_is_rate_limited_after_configured_burst() {
    let mut config = test_auth_config();
    config.authorize_requests_per_minute = 1;
    let state = test_auth_state_with_config(config).await;
    state
        .store
        .register_client(RegisteredClient {
            client_id: "client".to_string(),
            redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
            created_at: now_unix(),
        })
        .await
        .unwrap();
    let app = router(state);

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/authorize?response_type=code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&scope=lab&code_challenge=pkce&code_challenge_method=S256")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::FOUND);

    let second = app
        .oneshot(
            Request::builder()
                .uri("/authorize?response_type=code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=def&scope=lab&code_challenge=pkce&code_challenge_method=S256")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn browser_login_starts_upstream_flow_and_persists_return_to_state() {
    let state = test_auth_state().await;
    let app = router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login?return_to=%2Fgateways%2F%3Ftab%3Dlab")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FOUND);
    let location = Url::parse(
        response
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
    )
    .unwrap();
    let upstream_state = location
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.into_owned())
        .unwrap();
    let stored = state
        .store
        .take_browser_login_state(&upstream_state)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.return_to, "/gateways/?tab=lab");
}

#[tokio::test]
async fn browser_login_rejects_when_pending_oauth_state_cap_is_reached() {
    let mut config = test_auth_config();
    config.max_pending_oauth_states = 1;
    let state = test_auth_state_with_config(config).await;
    state
        .store
        .insert_browser_login_state(crate::types::BrowserLoginStateRow {
            state: "existing-login".to_string(),
            return_to: "/".to_string(),
            provider_code_verifier: "provider-verifier".to_string(),
            created_at: now_unix(),
            expires_at: now_unix() + 300,
        })
        .await
        .unwrap();

    let app = router(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/login?return_to=%2Fgateways%2F")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn callback_rejects_expired_or_mismatched_state() {
    let app = router(test_auth_state_with_mock_google().await);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/google/callback?state=bad-state&code=upstream-code")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
