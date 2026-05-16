use super::*;

#[test]
fn collection_name_accepts_safe_names() {
    for ok in ["cortex", "axon_v2", "my-collection", "a.b.c", "a"] {
        assert!(
            validate_collection_name(ok).is_ok(),
            "expected {ok:?} to pass"
        );
    }
}

#[test]
fn collection_name_rejects_path_and_url_unsafe_names() {
    for bad in [
        "",
        ".",
        "..",
        "../etc/passwd",
        "a/b",
        ".hidden",
        "trailing.",
        "a..b",
        "a?x=1",
        "a#frag",
        "a b",
        "a%2e%2e",
    ] {
        assert!(
            validate_collection_name(bad).is_err(),
            "expected {bad:?} to fail"
        );
    }
}

#[test]
fn collection_name_rejects_overlong() {
    assert!(validate_collection_name(&"a".repeat(256)).is_err());
}
