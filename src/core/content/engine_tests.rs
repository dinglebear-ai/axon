use super::*;

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
        openai_base_url: String::new(),
        openai_api_key: String::new(),
        openai_model: String::new(),
        llm_backend: crate::services::llm_backend::LlmBackendConfig::default(),
        custom_headers: vec![],
        render_mode: RenderMode::Chrome,
        chrome_remote_url: None, // ← no Chrome configured
        chrome_stealth: true,
        chrome_anti_bot: true,
        chrome_intercept: true,
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
