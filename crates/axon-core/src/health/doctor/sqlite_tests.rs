use super::*;
use httpmock::{Method::GET, MockServer};
use reqwest::header::{HeaderName, HeaderValue};

fn auth(name: HeaderName, value: &str) -> ProbeAuth {
    let mut value = HeaderValue::from_str(value).expect("valid test header");
    value.set_sensitive(true);
    ProbeAuth { name, value }
}

#[test]
fn exact_origin_accepts_only_matching_transport_host_and_effective_port() {
    assert!(same_origin(
        "https://tei.example/base",
        "https://tei.example:443/info"
    ));
    assert!(!same_origin(
        "https://tei.example",
        "http://tei.example/info"
    ));
    assert!(!same_origin(
        "https://tei.example:8443",
        "https://tei.example:9443/info"
    ));
    assert!(!same_origin(
        "https://tei.example",
        "https://gateway.example/info"
    ));
    assert!(!same_origin(
        "https://user@tei.example",
        "https://tei.example/info"
    ));
}

#[test]
fn provider_headers_are_attached_only_to_the_configured_origin() {
    let client = reqwest::Client::new();
    let qdrant = auth(HeaderName::from_static("api-key"), "qdrant-secret");
    let same_origin = authenticated_probe_request(
        &client,
        "https://qdrant.example:6333",
        "https://qdrant.example:6333/collections/cortex",
        Some(&qdrant),
    )
    .build()
    .expect("same-origin request");
    assert_eq!(same_origin.headers()["api-key"], "qdrant-secret");

    let cross_origin = authenticated_probe_request(
        &client,
        "https://qdrant.example:6333",
        "https://attacker.example/collections/cortex",
        Some(&qdrant),
    )
    .build()
    .expect("cross-origin request");
    assert!(!cross_origin.headers().contains_key("api-key"));
}

#[test]
fn malformed_provider_credentials_fail_without_exposing_the_value() {
    let secret = "secret\nleak";
    let error = ProbeAuth::from_value(
        "AXON_CHROME_BEARER_TOKEN",
        reqwest::header::AUTHORIZATION,
        "Bearer ",
        secret,
    )
    .err()
    .expect("a credential containing a newline must be rejected");

    assert!(error.contains("AXON_CHROME_BEARER_TOKEN"));
    assert!(!error.contains(secret));
    assert!(!error.contains("secret"));
}

#[tokio::test]
async fn tei_info_fallback_sends_bearer_auth_on_each_same_origin_probe() {
    let server = MockServer::start_async().await;
    let first = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/info")
                .header("authorization", "Bearer tei-secret");
            then.status(404);
        })
        .await;
    let second = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/v1/info")
                .header("authorization", "Bearer tei-secret");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"model_id":"gateway-model"}"#);
        })
        .await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("test client");
    let bearer = auth(reqwest::header::AUTHORIZATION, "Bearer tei-secret");

    let (info, detail) = probe_authenticated_tei_info(&server.base_url(), &client, &bearer).await;

    assert_eq!(
        info.and_then(|value| value["model_id"].as_str().map(str::to_string)),
        Some("gateway-model".to_string())
    );
    assert!(detail.is_some_and(|value| value.starts_with("/v1/info 200")));
    first.assert_async().await;
    second.assert_async().await;
}

#[tokio::test]
async fn qdrant_collection_info_sends_api_key() {
    let server = MockServer::start_async().await;
    let collection = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/collections/cortex")
                .header("api-key", "qdrant-secret");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"result":{"config":{"params":{"vectors":{"dense":{"size":768}}}}}}"#);
        })
        .await;
    let client = reqwest::Client::new();
    let api_key = auth(HeaderName::from_static("api-key"), "qdrant-secret");

    let result = probe_collection_info(&client, &server.base_url(), "cortex", Some(&api_key)).await;

    assert_eq!(result, (Some("named".to_string()), Some(768)));
    collection.assert_async().await;
}

#[tokio::test]
async fn authenticated_probe_rejects_redirects_without_leaking_credentials() {
    let source = MockServer::start_async().await;
    let destination = MockServer::start_async().await;
    let leaked = destination
        .mock_async(|when, then| {
            when.method(GET)
                .path("/stolen")
                .header("authorization", "Bearer chrome-secret");
            then.status(200);
        })
        .await;
    source
        .mock_async(|when, then| {
            when.method(GET)
                .path("/json/version")
                .header("authorization", "Bearer chrome-secret");
            then.status(302)
                .header("location", destination.url("/stolen"));
        })
        .await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("test client");
    let bearer = auth(reqwest::header::AUTHORIZATION, "Bearer chrome-secret");

    let result = probe_internal_http(
        &client,
        &source.base_url(),
        &["/json/version"],
        Some(&bearer),
    )
    .await;

    assert_eq!(result, (false, Some("http 302".to_string())));
    leaked.assert_calls_async(0).await;
}
