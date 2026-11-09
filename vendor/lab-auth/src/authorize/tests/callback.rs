use super::*;

#[tokio::test]
async fn browser_login_callback_sets_session_cookie_and_redirects_home() {
    let state = test_auth_state_with_mock_google().await;
    let app = router(state.clone());
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/login?return_to=%2Fgateways%2F")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let location = Url::parse(
        login
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

    let callback = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/auth/google/callback?state={upstream_state}&code=upstream-code"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(callback.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        callback.headers().get(header::LOCATION).unwrap(),
        "/gateways/"
    );
    let cookie = callback
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|value| value.to_str().ok())
        .unwrap();
    assert!(cookie.contains("lab_session="));
}

#[tokio::test]
async fn oauth_client_callback_redirects_with_access_denied_when_email_not_in_allowlist() {
    let mut config = test_auth_config();
    config.admin_email = "allowed@example.com".to_string();
    let base_state = test_auth_state_with_config(config).await;
    base_state
        .store
        .register_client(RegisteredClient {
            client_id: "client".to_string(),
            redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
            created_at: now_unix(),
        })
        .await
        .unwrap();
    // Pre-insert an authorization request (OAuth-client flow, not browser-login).
    base_state
        .store
        .insert_authorization_request(AuthorizationRequestRow {
            state: "good-state".to_string(),
            client_id: "client".to_string(),
            redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
            client_state: "client-abc".to_string(),
            scope: "lab".to_string(),
            provider_code_verifier: "provider-verifier".to_string(),
            code_challenge: "challenge".to_string(),
            code_challenge_method: "S256".to_string(),
            created_at: now_unix(),
            expires_at: now_unix() + 300,
        })
        .await
        .unwrap();

    let server = Box::leak(Box::new(MockServer::start().await));
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "google-access-token",
            "refresh_token": "refresh-token",
            "expires_in": 3600,
            "id_token": signed_test_id_token(), // email=user@example.com, not in allowlist
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/certs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(test_jwks()))
        .mount(server)
        .await;

    let google = GoogleProvider::new(
        "client-id".to_string(),
        "client-secret".to_string(),
        Url::parse("https://lab.example.com/auth/google/callback").unwrap(),
    )
    .unwrap()
    .with_endpoints(
        server.uri().parse::<Url>().unwrap(),
        server.uri().parse::<Url>().unwrap().join("/token").unwrap(),
    )
    .with_jwks_endpoint(server.uri().parse::<Url>().unwrap().join("/certs").unwrap());

    let state = AuthState::for_tests(
        (*base_state.config).clone(),
        base_state.store.clone(),
        (*base_state.signing_keys).clone(),
        google,
    );
    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/google/callback?state=good-state&code=upstream-code")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Must redirect (not 401) with error=access_denied and the original client state.
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    let redirect = Url::parse(location).unwrap();
    let params: std::collections::HashMap<_, _> = redirect.query_pairs().collect();
    assert_eq!(
        params.get("error").map(|v| v.as_ref()),
        Some("access_denied")
    );
    assert_eq!(params.get("state").map(|v| v.as_ref()), Some("client-abc"));
}

#[tokio::test]
async fn browser_login_callback_rejects_email_not_in_allowlist() {
    let mut config = test_auth_config();
    // "allowed@example.com" is permitted; the mock id_token returns
    // "user@example.com" → callback must be denied with 401.
    config.admin_email = "allowed@example.com".to_string();
    let base_state = test_auth_state_with_config(config).await;
    base_state
        .store
        .register_client(RegisteredClient {
            client_id: "client".to_string(),
            redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
            created_at: now_unix(),
        })
        .await
        .unwrap();

    let server = Box::leak(Box::new(MockServer::start().await));
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "google-access-token",
            "refresh_token": "refresh-token",
            "expires_in": 3600,
            "id_token": signed_test_id_token(),
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/certs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(test_jwks()))
        .mount(server)
        .await;

    let google = GoogleProvider::new(
        "client-id".to_string(),
        "client-secret".to_string(),
        Url::parse("https://lab.example.com/auth/google/callback").unwrap(),
    )
    .unwrap()
    .with_endpoints(
        server.uri().parse::<Url>().unwrap(),
        server.uri().parse::<Url>().unwrap().join("/token").unwrap(),
    )
    .with_jwks_endpoint(server.uri().parse::<Url>().unwrap().join("/certs").unwrap());

    let state = AuthState::for_tests(
        (*base_state.config).clone(),
        base_state.store.clone(),
        (*base_state.signing_keys).clone(),
        google,
    );
    let app = router(state.clone());

    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/login?return_to=%2F")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let location = Url::parse(
        login
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

    let callback = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/auth/google/callback?state={upstream_state}&code=upstream-code"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(callback.status(), StatusCode::UNAUTHORIZED);
}
