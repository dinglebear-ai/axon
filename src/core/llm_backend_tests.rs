use super::*;

#[test]
fn limiter_key_uses_configured_backend_model_not_request_override() {
    let backend = LlmBackendConfig {
        kind: LlmBackendKind::OpenAiCompat,
        openai_base_url: Some("http://127.0.0.1:8080/v1".to_string()),
        openai_model: Some("default-model".to_string()),
        configured: true,
        ..LlmBackendConfig::default()
    };
    let mut req = CompletionRequest::new("hello").model("override-model");
    req.backend = backend;

    assert_eq!(
        completion_limiter_key(&req),
        CompletionKey::OpenAi {
            base_url: "http://127.0.0.1:8080/v1".to_string(),
            model: "default-model".to_string(),
        }
    );
}

#[test]
fn limiter_key_falls_back_to_backend_model() {
    let backend = LlmBackendConfig {
        kind: LlmBackendKind::GeminiHeadless,
        gemini_cmd: "gemini".to_string(),
        gemini_model: Some("configured-model".to_string()),
        configured: true,
        ..LlmBackendConfig::default()
    };
    let mut req = CompletionRequest::new("hello");
    req.backend = backend;

    assert_eq!(
        completion_limiter_key(&req),
        CompletionKey::Gemini {
            cmd: "gemini".to_string(),
            model: "configured-model".to_string(),
        }
    );
}

// ── T-M6: LlmBackendKind alias-resolution tests ─────────────────────────────

#[test]
fn backend_kind_parses_empty_string_as_gemini() {
    assert_eq!(
        LlmBackendKind::parse(""),
        Ok(LlmBackendKind::GeminiHeadless)
    );
}

#[test]
fn backend_kind_parses_gemini_aliases() {
    for alias in ["gemini-headless", "gemini", "headless"] {
        assert_eq!(
            LlmBackendKind::parse(alias),
            Ok(LlmBackendKind::GeminiHeadless),
            "alias '{alias}' should resolve to GeminiHeadless"
        );
    }
}

#[test]
fn backend_kind_parses_openai_compat_aliases() {
    for alias in ["openai-compat", "openai_compat"] {
        assert_eq!(
            LlmBackendKind::parse(alias),
            Ok(LlmBackendKind::OpenAiCompat),
            "alias '{alias}' should resolve to OpenAiCompat"
        );
    }
}

#[test]
fn backend_kind_rejects_unknown_alias() {
    assert!(LlmBackendKind::parse("unknown-backend").is_err());
}

#[test]
fn backend_kind_parse_trims_whitespace() {
    assert_eq!(
        LlmBackendKind::parse("  gemini  "),
        Ok(LlmBackendKind::GeminiHeadless)
    );
    assert_eq!(
        LlmBackendKind::parse("  openai-compat  "),
        Ok(LlmBackendKind::OpenAiCompat)
    );
}
