use super::super::super::toml_config::{TomlProvider, load_toml_config_from_str};
use super::*;

fn provider(backend: &str, model: Option<&str>) -> TomlProvider {
    TomlProvider {
        backend: Some(backend.to_string()),
        model: model.map(str::to_string),
        ..TomlProvider::default()
    }
}

#[test]
fn codex_profile_routes_model_to_codex_slot() {
    let overlay =
        overlay_from_profile("c", &provider("codex-app-server", Some("gpt-5.5"))).unwrap();
    assert_eq!(overlay.backend.as_deref(), Some("codex-app-server"));
    assert_eq!(overlay.codex_model.as_deref(), Some("gpt-5.5"));
    assert!(overlay.gemini_model.is_none());
    assert!(overlay.openai_model.is_none());
}

#[test]
fn gemini_profile_routes_model_cmd_home() {
    let mut p = provider("gemini-headless", Some("gemini-3.1-flash"));
    p.cmd = Some("gemini".to_string());
    p.home = Some("/tmp/gh".to_string());
    let overlay = overlay_from_profile("g", &p).unwrap();
    assert_eq!(overlay.gemini_model.as_deref(), Some("gemini-3.1-flash"));
    assert_eq!(overlay.gemini_cmd.as_deref(), Some("gemini"));
    assert_eq!(overlay.gemini_home.as_deref(), Some("/tmp/gh"));
    assert!(overlay.codex_model.is_none());
}

#[test]
fn openai_profile_routes_base_url_and_api_key() {
    let mut p = provider("openai-compat", Some("gemma-4"));
    p.base_url = Some("http://127.0.0.1:8080/v1".to_string());
    p.api_key = Some("sk-xyz".to_string());
    let overlay = overlay_from_profile("o", &p).unwrap();
    assert_eq!(overlay.openai_model.as_deref(), Some("gemma-4"));
    assert_eq!(
        overlay.openai_base_url.as_deref(),
        Some("http://127.0.0.1:8080/v1")
    );
    assert_eq!(overlay.openai_api_key.as_deref(), Some("sk-xyz"));
}

#[test]
fn aliased_backend_codex_is_accepted() {
    let overlay = overlay_from_profile("c", &provider("codex", None)).unwrap();
    assert_eq!(overlay.backend.as_deref(), Some("codex"));
}

#[test]
fn missing_backend_is_an_error() {
    let err = overlay_from_profile("x", &TomlProvider::default()).unwrap_err();
    assert!(err.contains("missing a `backend`"));
}

#[test]
fn invalid_backend_is_an_error() {
    let err = overlay_from_profile("x", &provider("not-a-backend", None)).unwrap_err();
    assert!(err.contains("invalid backend"));
}

#[test]
fn blank_fields_are_dropped() {
    let mut p = provider("codex-app-server", Some("   "));
    p.cmd = Some("".to_string());
    let overlay = overlay_from_profile("c", &p).unwrap();
    assert!(overlay.codex_model.is_none());
    assert!(overlay.codex_cmd.is_none());
}

#[test]
fn flag_selects_named_profile() {
    let toml = load_toml_config_from_str(
        r#"
[providers.codex]
backend = "codex-app-server"
model = "gpt-5.5"

[providers.gem]
backend = "gemini-headless"
model = "gemini-3.1-flash"
"#,
    )
    .unwrap();
    let overlay = resolve_provider_overlay(&toml, Some("gem")).unwrap();
    assert_eq!(overlay.backend.as_deref(), Some("gemini-headless"));
    assert_eq!(overlay.gemini_model.as_deref(), Some("gemini-3.1-flash"));
}

#[test]
fn flag_for_unknown_profile_errors() {
    let toml = load_toml_config_from_str(
        r#"
[providers.codex]
backend = "codex-app-server"
"#,
    )
    .unwrap();
    let err = resolve_provider_overlay(&toml, Some("nope")).unwrap_err();
    assert!(err.contains("not defined under [providers.nope]"));
}

#[test]
fn toml_active_provider_selects_when_no_flag() {
    // Deterministic only when AXON_PROVIDER is not set in the environment.
    if std::env::var_os("AXON_PROVIDER").is_some() {
        return;
    }
    let toml = load_toml_config_from_str(
        r#"
[llm]
active-provider = "codex"

[providers.codex]
backend = "codex-app-server"
model = "gpt-5.5"
"#,
    )
    .unwrap();
    let overlay = resolve_provider_overlay(&toml, None).unwrap();
    assert_eq!(overlay.backend.as_deref(), Some("codex-app-server"));
    assert_eq!(overlay.codex_model.as_deref(), Some("gpt-5.5"));
}

#[test]
fn no_active_provider_yields_empty_overlay() {
    if std::env::var_os("AXON_PROVIDER").is_some() {
        return;
    }
    let toml = load_toml_config_from_str("").unwrap();
    let overlay = resolve_provider_overlay(&toml, None).unwrap();
    assert_eq!(overlay, ProviderOverlay::default());
}

#[test]
fn backend_from_overlay_uses_overlay_backend_before_env() {
    // overlay.backend is Some → returned before the AXON_LLM_BACKEND fallback is
    // even consulted, so an active profile's backend always wins over env.
    let overlay = ProviderOverlay {
        backend: Some("codex-app-server".to_string()),
        ..ProviderOverlay::default()
    };
    assert_eq!(
        backend_from_overlay(&overlay).unwrap(),
        LlmBackendKind::CodexAppServer
    );
}

#[test]
fn backend_from_overlay_accepts_backend_aliases() {
    let overlay = ProviderOverlay {
        backend: Some("codex".to_string()),
        ..ProviderOverlay::default()
    };
    assert_eq!(
        backend_from_overlay(&overlay).unwrap(),
        LlmBackendKind::CodexAppServer
    );
}

#[test]
fn effective_backend_kind_resolves_active_profile() {
    if std::env::var_os("AXON_PROVIDER").is_some() {
        return;
    }
    let toml = load_toml_config_from_str(
        r#"
[llm]
active-provider = "cdx"

[providers.cdx]
backend = "codex-app-server"
"#,
    )
    .unwrap();
    assert_eq!(
        effective_backend_kind(&toml, None).unwrap(),
        LlmBackendKind::CodexAppServer
    );
}

#[test]
fn effective_backend_kind_errors_on_broken_active_profile() {
    if std::env::var_os("AXON_PROVIDER").is_some() {
        return;
    }
    let toml = load_toml_config_from_str(
        r#"
[llm]
active-provider = "ghost"
"#,
    )
    .unwrap();
    let err = effective_backend_kind(&toml, None).unwrap_err();
    assert!(
        err.contains("not defined under [providers.ghost]"),
        "got: {err}"
    );
}

#[test]
#[serial_test::serial]
fn axon_provider_env_beats_toml_active_provider() {
    // The middle precedence tier: AXON_PROVIDER env > [llm] active-provider.
    let toml = load_toml_config_from_str(
        r#"
[llm]
active-provider = "gem"

[providers.gem]
backend = "gemini-headless"

[providers.cdx]
backend = "codex-app-server"
"#,
    )
    .unwrap();
    let prev = std::env::var("AXON_PROVIDER").ok();
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var("AXON_PROVIDER", "cdx");
    }
    let overlay = resolve_provider_overlay(&toml, None).unwrap();
    #[allow(unsafe_code)]
    match prev {
        Some(v) => unsafe { std::env::set_var("AXON_PROVIDER", v) },
        None => unsafe { std::env::remove_var("AXON_PROVIDER") },
    }
    assert_eq!(overlay.backend.as_deref(), Some("codex-app-server"));
}
