use spider_transformations::transformation::content::{
    ReturnFormat, SelectorConfiguration, TransformConfig, TransformInput, transform_content_input,
};
use std::sync::LazyLock;

pub const BOILERPLATE_SELECTORS: &[&str] = &[
    "[role=\"navigation\"]",
    "[role=\"banner\"]",
    "[role=\"contentinfo\"]",
    "[role=\"complementary\"]",
    "[role=\"search\"]",
    "[role=\"dialog\"]",
    "[role=\"alertdialog\"]",
    "[role=\"form\"]",
    "[aria-hidden=\"true\"]",
    "noscript",
    "iframe",
    "[hidden]",
    "[data-nosnippet]",
    "a[href=\"#content-area\"]",
    "#navbar",
    "#sidebar",
    "#footer",
    "#table-of-contents",
    "#search-bar-entry",
    "#search-bar-entry-mobile",
    "#page-context-menu",
    "#pagination",
    "#feedback-thumbs-up",
    "#feedback-thumbs-down",
    ".feedback-toolbar",
];

static TRANSFORM_CONFIG: LazyLock<TransformConfig> = LazyLock::new(|| TransformConfig {
    return_format: ReturnFormat::Markdown,
    // Readability (Mozilla-style article scoring) discards documentation pages
    // that lack <article> structure — doc sites with sidebar + nested divs score
    // too low and get stripped to just the title. main_content=true already
    // extracts <main>/<article>/role=main structurally without the scoring penalty.
    readability: false,
    // clean_html uses [class*='ad'] which matches Tailwind `shadow-*` classes
    // (sh**ad**ow contains "ad"). This wipes all shadow-styled elements from
    // Tailwind CSS sites (react.dev, shadcn.com, etc.), leaving only the title.
    // html2md ignores script/style content natively, so clean_html buys nothing.
    clean_html: false,
    main_content: true,
    filter_images: true,
    filter_svg: true,
});

pub fn build_transform_config() -> &'static TransformConfig {
    &TRANSFORM_CONFIG
}

/// Build a `SelectorConfiguration` from Config's `root_selector` / `exclude_selector`.
/// Returns `None` when neither selector is set (the common case).
pub fn build_selector_config(cfg: &crate::core::config::Config) -> Option<SelectorConfiguration> {
    if cfg.root_selector.is_none() && cfg.exclude_selector.is_none() {
        return None;
    }
    Some(SelectorConfiguration {
        root_selector: cfg.root_selector.clone(),
        exclude_selector: cfg.exclude_selector.clone(),
    })
}

/// Convert HTML to clean markdown with optional CSS selector scoping.
///
/// When `selector_config` is `Some`, spider scopes extraction to the root
/// selector and excludes elements matching the exclude selector — matching
/// Spider Cloud's official API behavior.
pub fn to_markdown(html: &str, selector_config: Option<&SelectorConfiguration>) -> String {
    bytes_to_markdown(html.as_bytes(), selector_config)
}

/// Convert HTML bytes to clean markdown with the crawl-wide transform policy.
///
/// This is the shared path for primary crawls, Chrome recovery, sitemap
/// backfill, and URL embed fetches so selector and boilerplate behavior cannot
/// drift between call sites.
pub fn bytes_to_markdown(
    html_bytes: &[u8],
    selector_config: Option<&SelectorConfiguration>,
) -> String {
    let input = TransformInput {
        url: None,
        content: html_bytes,
        screenshot_bytes: None,
        encoding: None,
        selector_config,
        ignore_tags: Some(BOILERPLATE_SELECTORS),
    };
    let raw = transform_content_input(input, &TRANSFORM_CONFIG);
    clean_markdown_whitespace(raw.trim())
}

/// Collapse runs of 3+ newlines to 2 and runs of 2+ horizontal spaces to 1.
/// Matches `spider_transformations::aho_clean_markdown` behavior.
pub fn clean_markdown_whitespace(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut newline_run = 0u32;
    let mut space_run = 0u32;

    for ch in md.chars() {
        if ch == '\n' {
            space_run = 0;
            newline_run += 1;
            if newline_run <= 2 {
                out.push('\n');
            }
        } else if ch == ' ' || ch == '\t' {
            newline_run = 0;
            space_run += 1;
            if space_run <= 1 {
                out.push(' ');
            }
        } else {
            newline_run = 0;
            space_run = 0;
            out.push(ch);
        }
    }
    out
}

/// Redact credentials from a URL, replacing username and password with `***`.
/// Returns `"***redacted***"` if the URL cannot be parsed.
pub fn redact_url(url: &str) -> String {
    use spider::url::Url;
    match Url::parse(url) {
        Ok(mut parsed) => {
            if !parsed.username().is_empty() || parsed.password().is_some() {
                let _ = parsed.set_username("***");
                let _ = parsed.set_password(Some("***"));
            }
            parsed.to_string()
        }
        Err(_) => "***redacted***".to_string(),
    }
}
