use axon_api::source::{SourceItemKey, SourceKind};

use crate::manifest::item_identity;

#[test]
fn item_identity_joins_relative_keys_for_source_families() {
    let cases = [
        (
            SourceKind::Web,
            "https://example.com/docs",
            "guide/install",
            "guide/install",
            "https://example.com/docs/guide/install",
        ),
        (
            SourceKind::Local,
            "local://lp_test",
            "src/lib.rs",
            "src/lib.rs",
            "local://lp_test/src/lib.rs",
        ),
        (
            SourceKind::Git,
            "github://jmagar/axon",
            "/src/lib.rs",
            "src/lib.rs",
            "github://jmagar/axon/src/lib.rs",
        ),
        (
            SourceKind::Registry,
            "pkg://crates/serde",
            "versions/1.0.0",
            "versions/1.0.0",
            "pkg://crates/serde/versions/1.0.0",
        ),
        (
            SourceKind::Feed,
            "feed://example.com/feed.xml",
            "entry/abc",
            "entry/abc",
            "feed://example.com/feed.xml/entry/abc",
        ),
        (
            SourceKind::Reddit,
            "reddit://r/rust",
            "thread/abc",
            "thread/abc",
            "reddit://r/rust/thread/abc",
        ),
        (
            SourceKind::Youtube,
            "youtube://video/dQw4w9WgXcQ",
            "captions/en",
            "captions/en",
            "youtube://video/dQw4w9WgXcQ/captions/en",
        ),
        (
            SourceKind::Session,
            "session://claude/abc123",
            "turn/1",
            "turn/1",
            "session://claude/abc123/turn/1",
        ),
        (
            SourceKind::Upload,
            "upload://art_123",
            "file/README.md",
            "file/README.md",
            "upload://art_123/file/README.md",
        ),
        (
            SourceKind::CliTool,
            "cli://repomix",
            "run/1",
            "run/1",
            "cli://repomix/run/1",
        ),
        (
            SourceKind::McpTool,
            "mcp://context7/tools/resolve-library-id",
            "call/1",
            "call/1",
            "mcp://context7/tools/resolve-library-id/call/1",
        ),
    ];

    for (kind, source_uri, raw_key, expected_key, expected_uri) in cases {
        let identity = item_identity(kind, source_uri, raw_key).unwrap();

        assert_eq!(identity.source_item_key, SourceItemKey::from(expected_key));
        assert_eq!(identity.canonical_uri, expected_uri);
    }
}

#[test]
fn item_identity_redacts_absolute_local_paths_from_public_keys() {
    let identity = item_identity(
        SourceKind::Local,
        "local://lp_test",
        "/home/jmagar/workspace/axon/src/main.rs",
    )
    .unwrap();

    assert_eq!(identity.source_item_key, SourceItemKey::from("src/main.rs"));
    assert_eq!(identity.canonical_uri, "local://lp_test/src/main.rs");
    assert!(!identity.canonical_uri.contains("/home/jmagar"));
    assert!(!identity.source_item_key.0.contains("/home/jmagar"));
}

#[test]
fn item_identity_keeps_absolute_local_paths_distinct_after_redaction() {
    let first = item_identity(SourceKind::Local, "local://lp_test", "/tmp/a/config.toml").unwrap();
    let second = item_identity(SourceKind::Local, "local://lp_test", "/tmp/b/config.toml").unwrap();

    assert_ne!(first.source_item_key, second.source_item_key);
    assert_ne!(first.canonical_uri, second.canonical_uri);
    assert!(!first.source_item_key.0.starts_with('/'));
    assert!(!second.source_item_key.0.starts_with('/'));
}

#[test]
fn item_identity_rejects_empty_keys() {
    let err = item_identity(SourceKind::Web, "https://example.com/docs", "   ")
        .expect_err("empty item keys are invalid");

    assert_eq!(err.code.0, "adapter.item_key.invalid");
    assert_eq!(err.stage, axon_error::ErrorStage::Normalizing);
}
