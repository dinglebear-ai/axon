use super::*;
use crate::content::deterministic::DeterministicExtractionEngine;
use spider_transformations::transformation::content::SelectorConfiguration;

// ── Regression guard: build_transform_config() safety rails ─────────────────

#[test]
fn build_transform_config_readability_is_false() {
    // Readability: true strips VitePress/sidebar layouts to just the page title —
    // confirmed production regression. Mozilla Readability scores doc sites with
    // sidebar + nested divs as low-quality (no <article> structure) and discards
    // them, producing 97% thin pages. main_content=true handles structural
    // extraction without the scoring penalty. DO NOT change this to true.
    let cfg = build_transform_config();
    assert!(
        !cfg.readability,
        "readability must stay false — setting it true causes a production regression \
         where VitePress/sidebar doc pages are stripped to just the page title (97% thin rate)"
    );
}

#[test]
fn build_transform_config_clean_html_is_false() {
    // clean_html: true uses [class*='ad'] which matches Tailwind shadow-* classes
    // (sh**ad**ow contains "ad"). This silently wipes all shadow-styled elements
    // from Tailwind CSS sites (react.dev, shadcn.com, etc.), leaving only the title.
    // html2md ignores <script>/<style> natively so clean_html buys nothing here.
    // DO NOT change this to true.
    let cfg = build_transform_config();
    assert!(
        !cfg.clean_html,
        "clean_html must stay false — [class*='ad'] matches Tailwind shadow-* classes, \
         silently wiping shadow-styled elements from Tailwind sites"
    );
}

#[test]
fn to_markdown_strips_aria_and_html5_boilerplate() {
    let html = r#"
        <html><body>
          <div role="banner">Site Header</div>
          <div role="navigation"><a href="/docs">Docs</a></div>
          <div role="search">Search...</div>
          <aside role="complementary">Sidebar links</aside>
          <div role="contentinfo">Footer links</div>
          <iframe src="https://example.com/embed"></iframe>
          <noscript>Please enable JavaScript</noscript>
          <div hidden>Hidden banner</div>
          <div data-nosnippet>Do not snippet me</div>
          <main>
            <h1>Specification</h1>
            <p>MCP defines how models communicate with tools.</p>
          </main>
        </body></html>
    "#;

    let markdown = to_markdown(html, None);

    assert!(markdown.contains("MCP defines how models communicate with tools."));
    assert!(!markdown.contains("Site Header"));
    assert!(!markdown.contains("Docs"));
    assert!(!markdown.contains("Search..."));
    assert!(!markdown.contains("Sidebar links"));
    assert!(!markdown.contains("Footer links"));
    assert!(!markdown.contains("Please enable JavaScript"));
    assert!(!markdown.contains("Hidden banner"));
    assert!(!markdown.contains("Do not snippet me"));
    assert!(!markdown.contains("example.com/embed"));
}

#[test]
fn to_markdown_preserves_selector_scoping_with_boilerplate_filtering() {
    let html = r#"
        <html><body>
          <div role="navigation">Top nav</div>
          <main id="content">
            <div class="intro">Keep me</div>
            <div class="skip">Drop me</div>
          </main>
        </body></html>
    "#;

    let selector_config = SelectorConfiguration {
        root_selector: Some("#content".to_string()),
        exclude_selector: Some(".skip".to_string()),
    };

    let markdown = to_markdown(html, Some(&selector_config));

    assert!(markdown.contains("Keep me"));
    assert!(!markdown.contains("Top nav"));
    assert!(!markdown.contains("Drop me"));
}

#[test]
fn to_markdown_strips_mintlify_navigation_chrome() {
    let html = r##"
        <html><body>
          <a href="#content-area">Skip to main content</a>
          <div id="navbar">
            <div>Search...</div>
            <div>Navigation</div>
            <div>Model Context Protocol home page</div>
          </div>
          <div id="sidebar">
            <a href="/docs/getting-started/intro">Documentation</a>
            <a href="/extensions/overview">Extensions</a>
          </div>
          <div id="table-of-contents">On this page</div>
          <div id="page-context-menu">Copy page</div>
          <main id="content-area">
            <h1>Specification</h1>
            <p>MCP is an open protocol that enables integration between tools and LLM apps.</p>
          </main>
          <div class="feedback-toolbar">
            <p>Was this page helpful?</p>
            <button id="feedback-thumbs-up">Yes</button>
            <button id="feedback-thumbs-down">No</button>
          </div>
          <div id="pagination">
            <a href="/prev">Previous</a>
            <a href="/next">Next</a>
          </div>
          <div id="footer">Was this page helpful</div>
        </body></html>
    "##;

    let markdown = to_markdown(html, None);

    assert!(markdown.contains("Specification"));
    assert!(markdown.contains("MCP is an open protocol"));
    assert!(!markdown.contains("Skip to main content"));
    assert!(!markdown.contains("Search..."));
    assert!(!markdown.contains("Navigation"));
    assert!(!markdown.contains("Model Context Protocol home page"));
    assert!(!markdown.contains("Documentation"));
    assert!(!markdown.contains("Extensions"));
    assert!(!markdown.contains("On this page"));
    assert!(!markdown.contains("Copy page"));
    assert!(!markdown.contains("Previous"));
    assert!(!markdown.contains("Next"));
    assert!(!markdown.contains("Was this page helpful"));
    assert!(!markdown.contains("Yes"));
    assert!(!markdown.contains("No"));
}

#[test]
fn test_redact_url_postgres() {
    let url = "postgresql://axon:secret123@localhost:5432/axon";
    let redacted = redact_url(url);
    assert!(!redacted.contains("secret123"));
    assert!(redacted.contains("***"));
}

#[test]
fn test_redact_url_amqp() {
    let url = "amqp://guest:guest@localhost:5672"; // gitleaks:allow -- test fixture for redact_url
    let redacted = redact_url(url);
    assert!(!redacted.contains("guest:guest"));
}

#[test]
fn test_redact_url_no_credentials() {
    let url = "http://example.com/path";
    assert_eq!(redact_url(url), url);
}

#[test]
fn test_redact_url_unparseable() {
    let result = redact_url("not a url at all !!!@#$");
    assert_eq!(result, "***redacted***");
}

#[test]
fn stable_url_filename_ignores_crawl_order() {
    let url = "https://agentclientprotocol.com/protocol/initialization";

    assert_eq!(url_to_stable_filename(url), url_to_stable_filename(url));
    assert_ne!(url_to_filename(url, 1), url_to_filename(url, 11));
}

#[test]
fn stable_url_filename_keeps_slug_and_hashes_identity() {
    let base = url_to_stable_filename("https://example.com/docs/page");
    let query = url_to_stable_filename("https://example.com/docs/page?version=2");
    let fragment = url_to_stable_filename("https://example.com/docs/page#section");

    assert!(base.starts_with("example-com-docs-page-"));
    assert!(base.ends_with(".md"));
    assert_ne!(base, query);
    assert_eq!(base, fragment);
}

#[test]
fn test_redact_url_username_only() {
    let url = "postgresql://admin@localhost:5432/db";
    let redacted = redact_url(url);
    assert!(!redacted.contains("admin@"));
    assert!(redacted.contains("***"));
}

#[test]
fn test_redact_url_redis_with_password() {
    let url = "redis://:mypassword@localhost:6379";
    let redacted = redact_url(url);
    assert!(!redacted.contains("mypassword"));
}

#[test]
fn test_default_engine_extracts_json_ld() {
    let html = r#"
        <html><head>
        <script type="application/ld+json">{"@type":"Article","headline":"Hello"}</script>
        </head></html>
    "#;
    let engine = DeterministicExtractionEngine::with_default_parsers();
    let page = engine.extract("https://example.com", html);
    assert!(!page.items.is_empty());
    assert!(page.parser_hits.iter().any(|x| x == "json-ld"));
}

#[test]
fn test_default_engine_dedups_identical_json_ld_items() {
    let html = r#"
        <html><head>
        <script type="application/ld+json">{"@type":"Article","headline":"Hello"}</script>
        <script type="application/ld+json">{"@type":"Article","headline":"Hello"}</script>
        </head></html>
    "#;
    let engine = DeterministicExtractionEngine::with_default_parsers();
    let page = engine.extract("https://example.com", html);
    assert_eq!(page.items.len(), 1);
}

// --- OpenGraphParser tests ---

#[test]
fn open_graph_parser_extracts_title() {
    let html = r#"<html><head>
        <meta property="og:title" content="My Page Title">
    </head></html>"#;
    let engine = DeterministicExtractionEngine::with_default_parsers();
    let page = engine.extract("https://example.com", html);
    assert!(page.parser_hits.iter().any(|h| h == "open-graph"));
    let item = page.items.iter().find(|v| v.get("og:title").is_some());
    assert!(item.is_some(), "og:title field should be present");
    assert_eq!(item.unwrap()["og:title"].as_str(), Some("My Page Title"));
}

#[test]
fn open_graph_parser_extracts_description() {
    let html = r#"<html><head>
        <meta property="og:description" content="A page about testing">
    </head></html>"#;
    let engine = DeterministicExtractionEngine::with_default_parsers();
    let page = engine.extract("https://example.com", html);
    assert!(page.parser_hits.iter().any(|h| h == "open-graph"));
    let item = page
        .items
        .iter()
        .find(|v| v.get("og:description").is_some());
    assert!(item.is_some(), "og:description field should be present");
    assert_eq!(
        item.unwrap()["og:description"].as_str(),
        Some("A page about testing")
    );
}

#[test]
fn open_graph_parser_returns_empty_for_no_og_tags() {
    let html = r#"<html><head>
        <meta name="description" content="Not an OG tag">
    </head></html>"#;
    let engine = DeterministicExtractionEngine::with_default_parsers();
    let page = engine.extract("https://example.com", html);
    assert!(!page.parser_hits.iter().any(|h| h == "open-graph"));
}

#[test]
fn open_graph_parser_injects_source_url_and_parser_fields() {
    let html = r#"<html><head>
        <meta property="og:title" content="Test">
    </head></html>"#;
    let engine = DeterministicExtractionEngine::with_default_parsers();
    let page = engine.extract("https://example.com/page", html);
    let item = page
        .items
        .iter()
        .find(|v| v.get("og:title").is_some())
        .unwrap();
    assert_eq!(
        item["_source_url"].as_str(),
        Some("https://example.com/page")
    );
    assert_eq!(item["_parser"].as_str(), Some("open-graph"));
}

// --- HtmlTableParser tests ---

#[test]
fn html_table_parser_detects_table_and_counts_rows() {
    let html = r#"<html><body>
        <table>
            <tr><th>Name</th><th>Value</th></tr>
            <tr><td>foo</td><td>1</td></tr>
            <tr><td>bar</td><td>2</td></tr>
        </table>
    </body></html>"#;
    let engine = DeterministicExtractionEngine::with_default_parsers();
    let page = engine.extract("https://example.com", html);
    assert!(page.parser_hits.iter().any(|h| h == "html-table"));
    let item = page.items.iter().find(|v| v.get("rows").is_some()).unwrap();
    assert_eq!(item["rows"].as_u64(), Some(3));
}

#[test]
fn html_table_parser_handles_multiple_tables() {
    let html = r#"<html><body>
        <table><tr><td>A</td></tr></table>
        <table><tr><td>B</td></tr><tr><td>C</td></tr></table>
    </body></html>"#;
    let engine = DeterministicExtractionEngine::with_default_parsers();
    let page = engine.extract("https://example.com", html);
    assert_eq!(
        page.items
            .iter()
            .filter(|v| v.get("rows").is_some())
            .count(),
        2,
        "should produce one item per table"
    );
}

#[test]
fn html_table_parser_returns_empty_for_no_tables() {
    let html = r#"<html><body><p>No tables here.</p></body></html>"#;
    let engine = DeterministicExtractionEngine::with_default_parsers();
    let page = engine.extract("https://example.com", html);
    assert!(!page.parser_hits.iter().any(|h| h == "html-table"));
}

#[test]
fn html_table_parser_injects_source_url_and_parser_fields() {
    let html = r#"<table><tr><td>data</td></tr></table>"#;
    let engine = DeterministicExtractionEngine::with_default_parsers();
    let page = engine.extract("https://example.com/data", html);
    let item = page.items.iter().find(|v| v.get("rows").is_some()).unwrap();
    assert_eq!(
        item["_source_url"].as_str(),
        Some("https://example.com/data")
    );
    assert_eq!(item["_parser"].as_str(), Some("html-table"));
}

// --- DeterministicExtractionEngine general tests ---

#[test]
fn deterministic_engine_handles_no_content() {
    let engine = DeterministicExtractionEngine::with_default_parsers();
    let page = engine.extract("https://example.com", "");
    assert!(page.items.is_empty());
    assert!(page.parser_hits.is_empty());
}

#[test]
fn deterministic_engine_handles_html_with_no_structured_data() {
    let html = r#"<html><body><h1>Hello</h1><p>Some plain text.</p></body></html>"#;
    let engine = DeterministicExtractionEngine::with_default_parsers();
    let page = engine.extract("https://example.com", html);
    assert!(page.items.is_empty());
}

#[test]
fn deterministic_engine_deduplicates_identical_json_ld_across_scripts() {
    let html = r#"
        <html><head>
        <script type="application/ld+json">{"@type":"Product","name":"Widget"}</script>
        <script type="application/ld+json">{"@type":"Product","name":"Widget"}</script>
        </head></html>
    "#;
    let engine = DeterministicExtractionEngine::with_default_parsers();
    let page = engine.extract("https://example.com", html);
    // Both json-ld scripts are identical — dedup should reduce to 1 item.
    let product_items: Vec<_> = page
        .items
        .iter()
        .filter(|v| v.get("@type").map(|t| t == "Product").unwrap_or(false))
        .collect();
    assert_eq!(
        product_items.len(),
        1,
        "identical items must be deduplicated"
    );
}

// --- canonicalize_url tests ---

#[test]
fn canonicalize_url_strips_default_http_port() {
    assert_eq!(
        canonicalize_url("http://example.com:80/path"),
        Some("http://example.com/path".to_string())
    );
}

#[test]
fn canonicalize_url_strips_default_https_port() {
    assert_eq!(
        canonicalize_url("https://example.com:443/page/"),
        Some("https://example.com/page".to_string())
    );
}

#[test]
fn canonicalize_url_keeps_non_default_port() {
    assert_eq!(
        canonicalize_url("https://example.com:8443/path"),
        Some("https://example.com:8443/path".to_string())
    );
}

#[test]
fn canonicalize_url_strips_fragment_and_trailing_slash() {
    assert_eq!(
        canonicalize_url("https://example.com/docs/#section"),
        Some("https://example.com/docs".to_string())
    );
}

#[test]
fn extract_anchor_hrefs_resolves_relative_links_against_base_url() {
    let html = r##"
        <a href="/project/docs/intro/">Intro</a>
        <a href="./api">API</a>
        <a href="#local">Local</a>
        <a href="javascript:void(0)">Ignore</a>
        <a href="mailto:test@example.com">Mail</a>
    "##;

    let links = extract_anchor_hrefs("https://example.github.io/project/", html, 10);

    assert_eq!(
        links,
        vec![
            "https://example.github.io/project/docs/intro/".to_string(),
            "https://example.github.io/project/api".to_string(),
        ]
    );
}

#[test]
fn extract_anchor_hrefs_ignores_non_anchor_href_attributes() {
    // href= on non-anchor elements (<link>, <base>, <area>) must NOT be captured;
    // only true <a> navigational links are.
    let html = r##"
        <link rel="stylesheet" href="https://cdn.example.com/style.css">
        <base href="https://base.example.com/">
        <area href="https://map.example.com/region">
        <a href="https://real.example.com/page">Real link</a>
    "##;

    let links = extract_anchor_hrefs("https://example.com/", html, 10);

    assert_eq!(links, vec!["https://real.example.com/page".to_string()]);
}

#[test]
fn extract_anchor_hrefs_ignores_data_href_inside_anchor() {
    // `data-href=` (and other `*href=` attributes) inside an <a> tag must not be
    // matched as the real href — only the genuine `href=` attribute is captured.
    let html = r##"<a data-href="https://tracker.example.com/x" href="https://real.example.com/page">Link</a>"##;

    let links = extract_anchor_hrefs("https://example.com/", html, 10);

    assert_eq!(links, vec!["https://real.example.com/page".to_string()]);
}
