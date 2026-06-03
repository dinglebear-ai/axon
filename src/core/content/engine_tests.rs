use super::*;
use crate::core::http::LoopbackGuard;
use httpmock::prelude::*;

/// When Chrome mode is requested but no chrome_remote_url is configured,
/// the extract engine must fall back to the HTTP path gracefully rather
/// than panicking or returning an error about a missing CDP connection.
#[tokio::test]
async fn extract_chrome_mode_without_remote_url_falls_back_to_http() {
    let engine = Arc::new(DeterministicExtractionEngine::default());
    let wcfg = ExtractWebConfig {
        start_url: "https://example.invalid".to_string(),
        prompt: "test".to_string(),
        limit: 1,
        llm_backend: crate::services::llm_backend::LlmBackendConfig::default(),
        custom_headers: vec![],
        render_mode: RenderMode::Chrome,
        chrome_remote_url: None, // ← no Chrome configured
        bypass_csp: false,
        accept_invalid_certs: false,
        request_timeout_ms: Some(1000),
        fetch_retries: 0,
        user_agent: None,
        chrome_network_idle_timeout_secs: 0,
    };
    // Should not panic. The URL is intentionally invalid so we get a network
    // error, which is expected. We only care it falls back to HTTP, not Chrome.
    let result = run_extract_with_engine(wcfg, engine).await;
    match result {
        Ok(_) => {} // unlikely with invalid URL, but fine
        Err(e) => {
            let msg = e.to_string();
            // Must NOT be a Chrome/CDP error
            assert!(
                !msg.contains("CDP") && !msg.contains("chrome_remote_url"),
                "Expected HTTP fallback error, got Chrome error: {msg}"
            );
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn extract_limit_one_uses_exact_single_url_path() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let root = server.mock(|when, then| {
        when.method(GET).path("/");
        then.status(200)
            .body(r#"<html><head><meta property="og:title" content="wrong root"></head></html>"#);
    });
    let target = server.mock(|when, then| {
        when.method(GET).path("/docs/page");
        then.status(200)
            .body(r#"<html><head><meta property="og:title" content="right target"></head></html>"#);
    });

    let engine = Arc::new(DeterministicExtractionEngine::with_default_parsers());
    let wcfg = ExtractWebConfig {
        start_url: format!("{}/docs/page", server.base_url()),
        prompt: String::new(),
        limit: 1,
        llm_backend: crate::services::llm_backend::LlmBackendConfig::default(),
        custom_headers: vec![],
        render_mode: RenderMode::Http,
        chrome_remote_url: None,
        bypass_csp: false,
        accept_invalid_certs: false,
        request_timeout_ms: Some(1000),
        fetch_retries: 0,
        user_agent: None,
        chrome_network_idle_timeout_secs: 0,
    };

    let run = run_extract_with_engine(wcfg, engine).await.unwrap();

    root.assert_calls(0);
    target.assert_calls(1);
    assert_eq!(run.pages_visited, 1);
    assert_eq!(run.results.len(), 1);
    assert_eq!(run.results[0]["og:title"], "right target");
}

#[test]
fn extract_single_page_gate_preserves_explicit_multi_page_limits() {
    assert!(!uses_single_page_extract_path(0));
    assert!(uses_single_page_extract_path(1));
    assert!(!uses_single_page_extract_path(2));
    assert!(!uses_single_page_extract_path(25));
}
