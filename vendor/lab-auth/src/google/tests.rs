use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use rsa::RsaPrivateKey;
use rsa::pkcs8::EncodePrivateKey;
use rsa::rand_core::{TryCryptoRng, TryRng, UnwrapErr};
use rsa::traits::PublicKeyParts;
use serde_json::json;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{AuthorizeUrlRequest, CachedGoogleJwks, GoogleJwk, GoogleJwks, GoogleProvider};

#[test]
fn google_authorize_url_includes_offline_access_prompt_and_pkce() {
    let provider = test_google_provider();
    let request = sample_request();
    let url = provider.authorize_url(&request).unwrap();
    assert!(url.as_str().contains("access_type=offline"));
    assert!(url.as_str().contains("prompt=consent"));
    assert!(url.as_str().contains("code_challenge="));
}

#[tokio::test]
async fn google_exchange_parses_subject_and_refresh_token() {
    let (provider, _server) = mocked_google_provider().await;
    let token = provider.exchange_code("code", "verifier").await.unwrap();
    assert_eq!(token.subject, "google-subject-123");
    assert_eq!(token.refresh_token.as_deref(), Some("refresh-token"));
}

#[tokio::test]
async fn google_exchange_rejects_unsigned_id_tokens() {
    let (provider, _server) = mocked_google_provider_with_id_token(test_id_token()).await;
    let error = provider
        .exchange_code("code", "verifier")
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("verify google id_token"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn google_exchange_rejects_wrong_audience_in_id_token() {
    let (provider, _server) =
        mocked_google_provider_with_id_token(signed_test_id_token("other-client", false, true))
            .await;
    let error = provider
        .exchange_code("code", "verifier")
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("invalid google id_token"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn google_exchange_rejects_expired_id_token() {
    let (provider, _server) =
        mocked_google_provider_with_id_token(signed_test_id_token("client-id", true, true)).await;
    let error = provider
        .exchange_code("code", "verifier")
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("invalid google id_token"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn google_exchange_rejects_wrong_issuer_in_id_token() {
    let (provider, _server) =
        mocked_google_provider_with_id_token(signed_test_id_token("client-id", false, false)).await;
    let error = provider
        .exchange_code("code", "verifier")
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("issuer"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn google_exchange_reuses_cached_jwks() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "google-access-token",
            "refresh_token": "refresh-token",
            "expires_in": 3600,
            "id_token": signed_test_id_token("client-id", false, true),
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/certs"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Cache-Control", "public, max-age=3600")
                .set_body_json(test_jwks()),
        )
        .mount(&server)
        .await;

    let provider = test_google_provider()
        .with_endpoints(
            server.uri().parse::<Url>().unwrap(),
            server.uri().parse::<Url>().unwrap().join("/token").unwrap(),
        )
        .with_jwks_endpoint(server.uri().parse::<Url>().unwrap().join("/certs").unwrap());

    provider.exchange_code("code-1", "verifier").await.unwrap();
    provider.exchange_code("code-2", "verifier").await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let jwks_requests = requests
        .iter()
        .filter(|request| request.url.path() == "/certs")
        .count();
    assert_eq!(jwks_requests, 1);
}

#[tokio::test]
async fn google_exchange_refreshes_jwks_when_cached_kid_is_missing() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "google-access-token",
            "refresh_token": "refresh-token",
            "expires_in": 3600,
            "id_token": signed_test_id_token("client-id", false, true),
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/certs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(test_jwks()))
        .mount(&server)
        .await;

    let provider = test_google_provider()
        .with_endpoints(
            server.uri().parse::<Url>().unwrap(),
            server.uri().parse::<Url>().unwrap().join("/token").unwrap(),
        )
        .with_jwks_endpoint(server.uri().parse::<Url>().unwrap().join("/certs").unwrap());
    *provider.jwks_cache.write().await = Some(CachedGoogleJwks {
        jwks: wrong_test_jwks(),
        expires_at: Instant::now() + Duration::from_secs(3600),
    });

    let exchange = provider.exchange_code("code", "verifier").await.unwrap();
    assert_eq!(exchange.subject, "google-subject-123");

    let requests = server.received_requests().await.unwrap();
    let jwks_requests = requests
        .iter()
        .filter(|request| request.url.path() == "/certs")
        .count();
    assert_eq!(jwks_requests, 1);
}

#[test]
fn parse_max_age_reads_cache_control_max_age() {
    assert_eq!(super::parse_max_age("public, max-age=3600"), Some(3600));
    assert_eq!(super::parse_max_age("no-cache"), None);
}

fn test_google_provider() -> GoogleProvider {
    GoogleProvider::new(
        "client-id".to_string(),
        "client-secret".to_string(),
        Url::parse("https://lab.example.com/auth/google/callback").unwrap(),
    )
    .unwrap()
}

async fn mocked_google_provider() -> (GoogleProvider, MockServer) {
    mocked_google_provider_with_id_token(signed_test_id_token("client-id", false, true)).await
}

async fn mocked_google_provider_with_id_token(id_token: String) -> (GoogleProvider, MockServer) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "google-access-token",
            "refresh_token": "refresh-token",
            "expires_in": 3600,
            "id_token": id_token,
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/certs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(test_jwks()))
        .mount(&server)
        .await;

    let provider = test_google_provider()
        .with_endpoints(
            server.uri().parse::<Url>().unwrap(),
            server.uri().parse::<Url>().unwrap().join("/token").unwrap(),
        )
        .with_jwks_endpoint(server.uri().parse::<Url>().unwrap().join("/certs").unwrap());
    // Keep the mock lease alive through the exchange; dropping it lets another
    // concurrent test recycle the server and replace its token response.
    (provider, server)
}

fn sample_request() -> AuthorizeUrlRequest {
    AuthorizeUrlRequest {
        state: "state-123".to_string(),
        scope: "lab".to_string(),
        code_challenge: "challenge".to_string(),
        code_challenge_method: "S256".to_string(),
    }
}

fn test_id_token() -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(br#"{"sub":"google-subject-123"}"#);
    format!("{header}.{payload}.")
}

fn signed_test_id_token(client_id: &str, expired: bool, valid_issuer: bool) -> String {
    let claims = json!({
        "iss": if valid_issuer { "https://accounts.google.com" } else { "https://evil.example.com" },
        "aud": client_id,
        "sub": "google-subject-123",
        "email": "user@example.com",
        "iat": (unix_now() - 10) as usize,
        "exp": if expired { (unix_now() - 3600) as usize } else { (unix_now() + 3600) as usize },
    });
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-kid".to_string());
    encode(&header, &claims, &test_encoding_key()).unwrap()
}

fn test_jwks() -> serde_json::Value {
    let key = test_rsa_key();
    let public_key = key.to_public_key();
    json!({
        "keys": [{
            "kid": "test-kid",
            "alg": "RS256",
            "kty": "RSA",
            "use": "sig",
            "n": URL_SAFE_NO_PAD.encode(public_key.n_bytes()),
            "e": URL_SAFE_NO_PAD.encode(public_key.e_bytes()),
        }]
    })
}

fn wrong_test_jwks() -> GoogleJwks {
    GoogleJwks {
        keys: vec![GoogleJwk {
            kid: "stale-kid".to_string(),
            alg: Some("RS256".to_string()),
            n: "stale-modulus".to_string(),
            e: "AQAB".to_string(),
        }],
    }
}

fn test_rsa_key() -> &'static RsaPrivateKey {
    static TEST_RSA_KEY: OnceLock<RsaPrivateKey> = OnceLock::new();
    TEST_RSA_KEY.get_or_init(|| {
        let mut rng = UnwrapErr(TestRng);
        RsaPrivateKey::new(&mut rng, 2048).unwrap()
    })
}

fn test_encoding_key() -> EncodingKey {
    let pem = test_rsa_key().to_pkcs8_pem(Default::default()).unwrap();
    EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap()
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

struct TestRng;

impl TryRng for TestRng {
    type Error = getrandom::Error;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes)?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        getrandom::fill(dst)
    }
}

impl TryCryptoRng for TestRng {}
