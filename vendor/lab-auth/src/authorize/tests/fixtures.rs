use super::*;

pub async fn test_auth_state() -> AuthState {
    test_auth_state_with_config(test_auth_config()).await
}

pub async fn test_auth_state_with_config(config: AuthConfig) -> AuthState {
    AuthState::new(config).await.unwrap()
}

pub fn test_auth_config() -> AuthConfig {
    let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    AuthConfig {
        mode: AuthMode::OAuth,
        public_url: Some(Url::parse("https://lab.example.com").unwrap()),
        sqlite_path: dir.path().join("auth.db"),
        key_path: dir.path().join("auth-jwt.pem"),
        bootstrap_secret: Some("bootstrap-secret".to_string()),
        allowed_client_redirect_uris: Vec::new(),
        // Matches the mock id_token email returned by signed_test_id_token,
        // so happy-path callback tests pass the allowlist check.
        admin_email: "user@example.com".to_string(),
        google: GoogleConfig {
            client_id: "client-id".to_string(),
            client_secret: "client-secret".to_string(),
            callback_path: "/auth/google/callback".to_string(),
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
            ..GoogleConfig::default()
        },
        ..AuthConfig::default()
    }
}

pub async fn test_auth_state_with_registered_client() -> AuthState {
    let state = test_auth_state().await;
    state
        .store
        .register_client(RegisteredClient {
            client_id: "client".to_string(),
            redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
            created_at: now_unix(),
        })
        .await
        .unwrap();
    state
}

pub(crate) async fn test_auth_state_with_mock_google() -> AuthState {
    let state = test_auth_state_with_registered_client().await;
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
    state
        .store
        .insert_authorization_request(AuthorizationRequestRow {
            state: "good-state".to_string(),
            client_id: "client".to_string(),
            redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
            client_state: "client-state".to_string(),
            scope: "lab".to_string(),
            provider_code_verifier: "provider-verifier".to_string(),
            code_challenge: "challenge".to_string(),
            code_challenge_method: "S256".to_string(),
            created_at: now_unix(),
            expires_at: now_unix() + 300,
        })
        .await
        .unwrap();
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
    AuthState::for_tests(
        (*state.config).clone(),
        state.store.clone(),
        (*state.signing_keys).clone(),
        google,
    )
}

pub(super) fn signed_test_id_token() -> String {
    let claims = json!({
        "iss": "https://accounts.google.com",
        "aud": "client-id",
        "sub": "google-subject-123",
        "email": "user@example.com",
        "email_verified": true,
        "iat": now_unix() as usize,
        "exp": (now_unix() + 3600) as usize,
    });
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-kid".to_string());
    encode(&header, &claims, &test_encoding_key()).unwrap()
}

pub(super) fn test_jwks() -> serde_json::Value {
    let key = test_rsa_key();
    let public_key = key.to_public_key();
    json!({
        "keys": [{
            "kid": "test-kid",
            "alg": "RS256",
            "kty": "RSA",
            "use": "sig",
            "n": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public_key.n_bytes()),
            "e": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public_key.e_bytes()),
        }]
    })
}

// Test RSA key — generated once per process via LazyLock to avoid the
// ~1s keygen cost being paid N times. The private key material only ever
// exists in memory during tests; it is never written to disk or committed.
//
// PRRT_kwDORS2O8s6AnbM5: replaced hardcoded PEM literal with runtime
// keygen so secret-scanners no longer flag this file.
static TEST_RSA_KEY: std::sync::LazyLock<rsa::RsaPrivateKey> = std::sync::LazyLock::new(|| {
    use rsa::rand_core::{TryCryptoRng, TryRng, UnwrapErr};
    #[derive(Default, Clone, Copy)]
    struct SystemRng;
    impl TryRng for SystemRng {
        type Error = getrandom::Error;
        fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
            let mut b = [0u8; 4];
            self.try_fill_bytes(&mut b)?;
            Ok(u32::from_le_bytes(b))
        }
        fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
            let mut b = [0u8; 8];
            self.try_fill_bytes(&mut b)?;
            Ok(u64::from_le_bytes(b))
        }
        fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
            getrandom::fill(dst)
        }
    }
    impl TryCryptoRng for SystemRng {}
    let mut rng = UnwrapErr(SystemRng);
    RsaPrivateKey::new(&mut rng, 2048).expect("test RSA key generation failed")
});

pub(super) fn test_rsa_key() -> &'static RsaPrivateKey {
    &TEST_RSA_KEY
}

pub(super) fn test_encoding_key() -> EncodingKey {
    let pem = test_rsa_key().to_pkcs8_pem(LineEnding::LF).unwrap();
    EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap()
}
