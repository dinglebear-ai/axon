use super::*;
use axon_core::config::Config;
use axon_core::http::LoopbackGuard;
use httpmock::prelude::*;

/// The page under test links to many other pages on the same site, proving
/// `crawl_for_source_page` performs a true single-page fetch: no link
/// following, exactly one manifest item / markdown file regardless of how
/// many links the page contains.
const MANY_LINKS_HTML: &str = r#"<html><body>
<p>This page has plenty of prose content so it clears the default thin-page
threshold of two hundred characters and is not filtered out by the
drop-thin-markdown guard that every acquisition path in Axon applies before
writing a page to disk.</p>
<a href="/page-1">one</a>
<a href="/page-2">two</a>
<a href="/page-3">three</a>
<a href="/page-4">four</a>
<a href="/page-5">five</a>
</body></html>"#;

fn test_cfg(output_dir: std::path::PathBuf) -> Config {
    Config {
        output_dir,
        ..Config::test_default()
    }
}

#[tokio::test]
async fn page_scope_acquires_exactly_one_item_from_a_many_link_page() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/root");
            then.status(200)
                .header("content-type", "text/html")
                .body(MANY_LINKS_HTML);
        })
        .await;

    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = test_cfg(tmp.path().to_path_buf());
    let url = format!("{}/root", server.base_url());

    let result = crawl_for_source_page(&cfg, &url)
        .await
        .expect("single-page acquisition should succeed");

    mock.assert_hits_async(1).await; // exactly one HTTP fetch — no link following
    assert_eq!(result.pages_seen, 1);
    assert_eq!(result.markdown_files, 1);

    let manifest = tokio::fs::read_to_string(&result.manifest_path)
        .await
        .expect("manifest should exist");
    let lines: Vec<&str> = manifest.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        1,
        "page scope must acquire exactly one manifest item, got: {manifest}"
    );
    let entry: serde_json::Value = serde_json::from_str(lines[0]).expect("valid manifest json");
    assert_eq!(
        entry["relative_path"]
            .as_str()
            .unwrap()
            .contains("markdown/"),
        true
    );

    let markdown_path = result.markdown_root.join(
        entry["relative_path"]
            .as_str()
            .expect("relative_path field"),
    );
    assert!(
        tokio::fs::try_exists(&markdown_path).await.unwrap_or(false),
        "markdown file referenced by the manifest must exist on disk"
    );
}

#[tokio::test]
async fn page_scope_drops_thin_pages_and_writes_zero_items() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/thin");
            then.status(200)
                .header("content-type", "text/html")
                .body("<html><body>hi</body></html>");
        })
        .await;

    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = test_cfg(tmp.path().to_path_buf());
    assert!(
        cfg.drop_thin_markdown,
        "test assumes the default drop-thin-markdown guard is enabled"
    );
    let url = format!("{}/thin", server.base_url());

    let result = crawl_for_source_page(&cfg, &url)
        .await
        .expect("thin single-page acquisition should still succeed (zero items)");

    mock.assert_hits_async(1).await;
    assert_eq!(result.pages_seen, 1);
    assert_eq!(
        result.markdown_files, 0,
        "thin page must not be written as a markdown file"
    );

    let manifest = tokio::fs::read_to_string(&result.manifest_path)
        .await
        .expect("manifest file should still be written (empty)");
    assert!(
        manifest.trim().is_empty(),
        "thin page must not produce a manifest entry, got: {manifest}"
    );
}
