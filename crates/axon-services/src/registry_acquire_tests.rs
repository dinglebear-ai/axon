use super::*;

#[test]
fn parses_each_registry() {
    assert_eq!(
        parse_registry_target("pkg:npm/left-pad").unwrap(),
        ("npm".to_string(), "left-pad".to_string())
    );
    assert_eq!(
        parse_registry_target("pkg:pypi/requests").unwrap(),
        ("pypi".to_string(), "requests".to_string())
    );
    assert_eq!(
        parse_registry_target("pkg:crates/serde").unwrap(),
        ("crates".to_string(), "serde".to_string())
    );
    assert_eq!(
        parse_registry_target("pkg://crates/serde").unwrap(),
        ("crates".to_string(), "serde".to_string())
    );
    assert_eq!(
        parse_registry_target("npm:left-pad").unwrap(),
        ("npm".to_string(), "left-pad".to_string())
    );
    assert_eq!(
        parse_registry_target("pypi:FastAPI").unwrap(),
        ("pypi".to_string(), "FastAPI".to_string())
    );
}

#[test]
fn scoped_npm_package_keeps_slash_in_name() {
    // The FIRST `/` separates registry from package; a scoped package's own `/`
    // must survive in the package name.
    assert_eq!(
        parse_registry_target("pkg:npm/@scope/name").unwrap(),
        ("npm".to_string(), "@scope/name".to_string())
    );
}

#[test]
fn registry_is_case_insensitive_and_trimmed() {
    assert_eq!(
        parse_registry_target("  pkg:NPM/left-pad  ").unwrap(),
        ("npm".to_string(), "left-pad".to_string())
    );
}

#[test]
fn is_registry_target_matches_valid_and_rejects_others() {
    assert!(is_registry_target("pkg:npm/left-pad"));
    assert!(is_registry_target("pkg://npm/left-pad"));
    assert!(is_registry_target("npm:left-pad"));
    assert!(is_registry_target("pypi:requests"));
    assert!(is_registry_target("pkg:crates/serde"));
    // Not a registry target.
    assert!(!is_registry_target("left-pad"));
    assert!(!is_registry_target("https://registry.npmjs.org/left-pad"));
    assert!(!is_registry_target("r/rust"));
    assert!(!is_registry_target("session:claude:/a/b.jsonl"));
}

#[test]
fn unknown_registry_is_rejected() {
    let err = parse_registry_target("pkg:cargo/serde").unwrap_err();
    assert!(err.contains("unknown registry"), "got: {err}");
    assert!(!is_registry_target("pkg:cargo/serde"));
}

#[test]
fn missing_package_is_rejected() {
    assert!(parse_registry_target("pkg:npm").is_err());
    assert!(parse_registry_target("pkg:npm/").is_err());
    assert!(parse_registry_target("pkg:npm/   ").is_err());
}

#[test]
fn missing_prefix_is_rejected() {
    assert!(parse_registry_target("npm/left-pad").is_err());
}

#[test]
fn cache_path_is_deterministic_and_target_specific() {
    // Same target -> same path (stable across calls).
    let a = registry_cache_path("npm", "left-pad");
    let b = registry_cache_path("npm", "left-pad");
    assert_eq!(a, b);
    // Under the axon-registry cache dir with a .json extension.
    assert!(a.starts_with(std::env::temp_dir().join("axon-registry")));
    assert_eq!(a.extension().and_then(|e| e.to_str()), Some("json"));
    // Different registry OR package -> different path.
    assert_ne!(a, registry_cache_path("pypi", "left-pad"));
    assert_ne!(a, registry_cache_path("npm", "right-pad"));
}

#[test]
fn metadata_url_encodes_scoped_npm_slash() {
    // A scoped npm package's `/` is percent-encoded so it stays one path segment.
    let url = metadata_url("npm", "@scope/name").unwrap();
    assert_eq!(url, "https://registry.npmjs.org/@scope%2Fname");
    assert_eq!(
        metadata_url("pypi", "requests").unwrap(),
        "https://pypi.org/pypi/requests/json"
    );
    assert_eq!(
        metadata_url("crates", "serde").unwrap(),
        "https://crates.io/api/v1/crates/serde"
    );
}

/// The fetch path must SSRF-guard: `build_client` returns an SSRF-guarded
/// client, and the metadata URLs are the public registry hosts. This test
/// asserts the deterministic-path + empty-package guards without any network.
#[tokio::test]
async fn fetch_rejects_empty_package_before_network() {
    let err = fetch_registry_dump("npm", "   ").await.unwrap_err();
    assert!(
        err.to_string().contains("non-empty package name"),
        "expected empty-package guard, got: {err}"
    );
}
