use axon_api::source::*;

use super::*;

#[tokio::test]
async fn fake_adapter_providers_cover_search_fetch_render_and_capture() {
    let fake = FakeAdapterProviders::new();
    assert_eq!(
        fake.search(SearchRequest {
            query: "axon".to_string(),
            limit: 1,
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap()
        .results
        .len(),
        1
    );

    let fetched = fake
        .fetch(FetchRequest {
            uri: "https://example.test".to_string(),
            method: "GET".to_string(),
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            body: None,
            timeout_ms: Some(1000),
            max_bytes: Some(1024),
            credential_refs: Vec::new(),
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap();
    assert_eq!(fetched.status, 200);
    assert_eq!(fetched.final_uri, "https://example.test");
    assert_eq!(
        fetched.fetched_at,
        Timestamp("2026-07-01T00:00:00Z".to_string())
    );

    let rendered = fake
        .render(RenderRequest {
            uri: "https://example.test".to_string(),
            mode: RenderMode::Http,
            timeout_ms: Some(1000),
            wait_ms: None,
            automation_script: None,
            credential_refs: Vec::new(),
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap();
    assert_eq!(rendered.markdown, "fake render");
    assert_eq!(rendered.render_mode, RenderMode::Http);

    let captured = fake
        .capture(NetworkCaptureRequest {
            uri: "https://example.test".to_string(),
            include_request_headers: true,
            include_response_headers: true,
            include_bodies: false,
            timeout_ms: Some(1000),
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap();
    assert!(captured.entries.is_empty());
    assert_eq!(
        NetworkCaptureProvider::capabilities(&fake)
            .await
            .unwrap()
            .provider_kind,
        ProviderKind::NetworkCapture
    );
}
