use axon_api::source::*;

use crate::boundary::FakeAdapterProviders;

use super::*;

fn item(uri: &str) -> ManifestItem {
    ManifestItem {
        source_id: SourceId::from("src_web_acquire_test"),
        source_item_key: SourceItemKey::from("docs/intro"),
        canonical_uri: uri.to_string(),
        item_kind: ItemKind::WebPage,
        content_kind: None,
        display_path: Some("docs/intro".to_string()),
        parent_key: None,
        size_bytes: None,
        content_hash: None,
        mtime: None,
        version: None,
        fetch_plan: None,
        metadata: MetadataMap::new(),
        graph_hints: Vec::new(),
    }
}

#[tokio::test]
async fn http_mode_calls_fetch_only_and_defaults_content_kind_to_html() {
    let providers = FakeAdapterProviders::new();
    let acquired = acquire_item(
        &providers,
        &providers,
        &item("https://example.com/docs/intro"),
        RenderMode::Http,
        200,
    )
    .await
    .unwrap();

    assert_eq!(providers.calls().await, vec!["fetch"]);
    assert_eq!(
        acquired.manifest_item.content_kind,
        Some(ContentKind::Html)
    );
    assert!(matches!(acquired.content_ref, ContentRef::InlineText { .. }));
    assert_eq!(acquired.metadata["web_render_mode"], "http");
}

#[tokio::test]
async fn chrome_mode_calls_render_once() {
    let providers = FakeAdapterProviders::new();
    let acquired = acquire_item(
        &providers,
        &providers,
        &item("https://example.com/docs/intro"),
        RenderMode::Chrome,
        200,
    )
    .await
    .unwrap();

    assert_eq!(providers.calls().await, vec!["render"]);
    assert_eq!(
        acquired.manifest_item.content_kind,
        Some(ContentKind::Markdown)
    );
    assert_eq!(acquired.metadata["web_fetch_method"], "chrome_render");
}

#[tokio::test]
async fn auto_switch_keeps_single_render_when_not_thin() {
    let providers = FakeAdapterProviders::new();
    // The fake's fixed "fake render" body (11 chars) is not thin against a
    // low threshold, so no Chrome re-render should occur.
    let acquired = acquire_item(
        &providers,
        &providers,
        &item("https://example.com/docs/intro"),
        RenderMode::AutoSwitch,
        5,
    )
    .await
    .unwrap();

    assert_eq!(providers.calls().await, vec!["render"]);
    assert_eq!(acquired.metadata["web_fetch_method"], "auto_switch_http");
}

#[tokio::test]
async fn auto_switch_re_renders_with_chrome_when_thin() {
    let providers = FakeAdapterProviders::new();
    // The fake's fixed "fake render" body (11 chars) is thin against a high
    // threshold, so a second (Chrome) render must occur.
    let acquired = acquire_item(
        &providers,
        &providers,
        &item("https://example.com/docs/intro"),
        RenderMode::AutoSwitch,
        1000,
    )
    .await
    .unwrap();

    assert_eq!(providers.calls().await, vec!["render", "render"]);
    assert_eq!(acquired.metadata["web_fetch_method"], "auto_switch_chrome");
}

#[tokio::test]
async fn http_mode_propagates_fetch_errors() {
    let providers = FakeAdapterProviders::new().with_mode(crate::boundary::FakeAdapterMode::Fatal);
    let err = acquire_item(
        &providers,
        &providers,
        &item("https://example.com/docs/intro"),
        RenderMode::Http,
        200,
    )
    .await
    .unwrap_err();

    assert!(!err.code.to_string().is_empty());
}
