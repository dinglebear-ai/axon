use super::*;

#[test]
fn detect_target_upper_snake_routes_to_env() {
    assert_eq!(detect_target("QDRANT_URL", false, false).unwrap(), Target::Env);
    assert_eq!(detect_target("GITHUB_TOKEN", false, false).unwrap(), Target::Env);
    assert_eq!(
        detect_target("AXON_HEADLESS_GEMINI_MODEL", false, false).unwrap(),
        Target::Env
    );
}

#[test]
fn detect_target_allows_underscore_prefixed_env_keys() {
    assert_eq!(detect_target("_MY_VAR", false, false).unwrap(), Target::Env);
    assert_eq!(detect_target("__INTERNAL", false, false).unwrap(), Target::Env);
}

#[test]
fn detect_target_dotted_lowercase_routes_to_toml() {
    assert_eq!(
        detect_target("ask.cache.enabled", false, false).unwrap(),
        Target::Toml
    );
    assert_eq!(
        detect_target("workers.embed-lanes", false, false).unwrap(),
        Target::Toml
    );
}

#[test]
fn detect_target_ambiguous_key_errors() {
    let err = detect_target("someKey", false, false).unwrap_err();
    assert!(err.to_string().contains("cannot infer target"));
}

#[test]
fn detect_target_force_env_overrides_heuristic() {
    assert_eq!(
        detect_target("ask.cache.enabled", true, false).unwrap(),
        Target::Env
    );
}

#[test]
fn detect_target_force_toml_overrides_heuristic() {
    assert_eq!(
        detect_target("QDRANT_URL", false, true).unwrap(),
        Target::Toml
    );
}

#[test]
fn detect_target_both_flags_is_error() {
    let err = detect_target("QDRANT_URL", true, true).unwrap_err();
    assert!(err.to_string().contains("mutually exclusive"));
}
