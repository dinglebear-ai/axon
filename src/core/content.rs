mod deterministic;
mod engine;

#[cfg(test)]
mod tests;

pub use deterministic::{
    DeterministicExtractionEngine, DeterministicParser, ExtractRun, ExtractionMetrics,
    PageExtraction,
};
pub use engine::{ExtractWebConfig, run_extract_with_engine};

use super::config::Config;
use sha2::{Digest, Sha256};
use spider::url::Url;
use spider_transformations::transformation::content::{
    ReturnFormat, SelectorConfiguration, TransformConfig, TransformInput, transform_content_input,
};
use std::collections::HashSet;
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

pub fn url_to_domain(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
        .replace(['[', ']', ':'], "_")
}

pub fn build_transform_config() -> &'static TransformConfig {
    &TRANSFORM_CONFIG
}

/// Build a `SelectorConfiguration` from Config's `root_selector` / `exclude_selector`.
/// Returns `None` when neither selector is set (the common case).
pub fn build_selector_config(cfg: &Config) -> Option<SelectorConfiguration> {
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

pub fn url_to_filename(url: &str, idx: u32) -> String {
    let parsed = Url::parse(url).ok();
    let host = parsed
        .as_ref()
        .and_then(|u| u.host_str())
        .unwrap_or("unknown-host");
    let path = parsed.as_ref().map(|u| u.path()).unwrap_or("/unknown-path");

    let stem_raw = format!("{host}{path}");
    let stem: String = stem_raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(80)
        .collect();

    format!("{:04}-{stem}.md", idx)
}

pub fn url_to_stable_filename(url: &str) -> String {
    let parsed = Url::parse(url).ok();
    let host = parsed
        .as_ref()
        .and_then(|u| u.host_str())
        .unwrap_or("unknown-host");
    let path = parsed.as_ref().map(|u| u.path()).unwrap_or("/unknown-path");

    let stem_raw = format!("{host}{path}");
    let stem = sanitized_filename_stem(&stem_raw);
    let identity = canonical_url_identity(url, parsed);
    let hash = Sha256::digest(identity.as_bytes());
    let short_hash = &hex::encode(hash)[..12];

    format!("{stem}-{short_hash}.md")
}

fn canonical_url_identity(url: &str, parsed: Option<Url>) -> String {
    parsed
        .map(|mut parsed| {
            parsed.set_fragment(None);
            parsed.to_string()
        })
        .unwrap_or_else(|| url.to_string())
}

fn sanitized_filename_stem(raw: &str) -> String {
    let mut stem = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(80)
        .collect::<String>();

    while stem.ends_with('-') {
        stem.pop();
    }

    if stem.is_empty() {
        "unknown".to_string()
    } else {
        stem
    }
}

pub fn find_between<'a>(haystack: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let s = haystack.find(start)? + start.len();
    let e = haystack[s..].find(end)? + s;
    Some(haystack[s..e].trim())
}

pub fn extract_meta_description(html: &str) -> Option<String> {
    // Limit search to <head> (≤8 KB) to avoid scanning the full document.
    let head_end = html
        .find("</head>")
        .or_else(|| html.find("</HEAD>"))
        .unwrap_or(html.len().min(8192));
    // Use .get() instead of direct index to avoid a panic when head_end falls
    // on a UTF-8 multi-byte boundary (possible when the 8192-byte default is used).
    let head = html.get(..head_end).unwrap_or(html);

    // Case-insensitive search without allocating a lowercase copy of the head.
    let head_bytes = head.as_bytes();
    let marker = b"name=\"description\"";
    let idx = head_bytes
        .windows(marker.len())
        .position(|w| w.eq_ignore_ascii_case(marker))?;
    let after_marker = &head_bytes[idx..];
    let content_marker = b"content=\"";
    let content_rel = after_marker
        .windows(content_marker.len())
        .position(|w| w.eq_ignore_ascii_case(content_marker))?;
    let content_start = idx + content_rel + content_marker.len();
    let rest = head.get(content_start..)?;
    let end = rest.find('"')?;
    Some(rest.get(..end)?.to_string())
}

pub fn extract_links(html: &str, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut pos = 0usize;
    let bytes = html.as_bytes();

    // Only extract href= values that appear inside <a ...> tags.
    // Two-pass approach: find each <a tag opening, then extract href= within it.
    while pos < html.len() {
        // Find the next <a tag (must be followed by whitespace or end-of-tag).
        let Some(a_rel) = html[pos..].find("<a") else {
            break;
        };
        let a_start = pos + a_rel;
        let after_a = a_start + 2;

        // Verify this is actually an <a tag and not e.g. <area, <aside — the
        // character after "<a" must be whitespace or ">".
        let next_byte = bytes.get(after_a).copied();
        if !matches!(
            next_byte,
            Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') | Some(b'>')
        ) {
            pos = after_a;
            continue;
        }

        // Find the closing > of this opening tag.
        let Some(tag_end_rel) = html[after_a..].find('>') else {
            break;
        };
        let tag_body = &html[after_a..after_a + tag_end_rel];

        // Now search for href=" within this tag body only.
        let mut tag_pos = 0usize;
        while let Some(href_rel) = tag_body[tag_pos..].find("href=\"") {
            let value_start = tag_pos + href_rel + 6;
            let remain = &tag_body[value_start..];
            let Some(end_rel) = remain.find('"') else {
                break;
            };
            let link = remain[..end_rel].trim();
            if (link.starts_with("http://") || link.starts_with("https://"))
                && seen.insert(link.to_string())
            {
                out.push(link.to_string());
                if out.len() >= limit {
                    return out;
                }
            }
            tag_pos = value_start + end_rel + 1;
        }

        pos = after_a + tag_end_rel + 1;
    }
    out
}

pub fn extract_anchor_hrefs(base_url: &str, html: &str, limit: usize) -> Vec<String> {
    let Some(base) = Url::parse(base_url).ok() else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut pos = 0usize;

    while let Some(rel) = html[pos..].find("href=") {
        let marker = pos + rel + 5;
        let Some(quote) = html[marker..].chars().next() else {
            break;
        };

        if quote != '"' && quote != '\'' {
            pos = marker;
            continue;
        }

        let value_start = marker + quote.len_utf8();
        let remain = &html[value_start..];
        let Some(value_end_rel) = remain.find(quote) else {
            break;
        };

        let raw = remain[..value_end_rel].trim();
        pos = value_start + value_end_rel + quote.len_utf8();

        if raw.is_empty()
            || raw.starts_with('#')
            || raw.starts_with("javascript:")
            || raw.starts_with("mailto:")
        {
            continue;
        }

        let Ok(resolved) = base.join(raw) else {
            continue;
        };

        match resolved.scheme() {
            "http" | "https" => {
                let link = resolved.to_string();
                if seen.insert(link.clone()) {
                    out.push(link);
                    if out.len() >= limit {
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    out
}

pub fn extract_loc_values(xml: &str) -> Vec<String> {
    // Case-insensitive search without cloning the full document (which can be 1–5 MB).
    // The sitemap spec mandates lowercase, but real-world feeds sometimes use <LOC>.
    const OPEN: &[u8] = b"<loc>";
    const CLOSE: &[u8] = b"</loc>";
    let bytes = xml.as_bytes();
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while cursor + OPEN.len() <= bytes.len() {
        let Some(rel) = bytes[cursor..]
            .windows(OPEN.len())
            .position(|w| w.eq_ignore_ascii_case(OPEN))
        else {
            break;
        };
        let start_idx = cursor + rel + OPEN.len();
        let Some(end_rel) = bytes[start_idx..]
            .windows(CLOSE.len())
            .position(|w| w.eq_ignore_ascii_case(CLOSE))
        else {
            break;
        };
        let end_idx = start_idx + end_rel;
        let value = xml[start_idx..end_idx].trim();
        if !value.is_empty() {
            out.push(value.replace("&amp;", "&"));
        }
        cursor = end_idx + CLOSE.len();
    }
    out
}

/// Find the value between `open` and `close` tags (case-insensitive) within `xml`.
/// Returns `None` if either tag is absent or the content is empty after trimming.
fn extract_between_tags(xml: &str, open: &[u8], close: &[u8]) -> Option<String> {
    let bytes = xml.as_bytes();
    let start = bytes
        .windows(open.len())
        .position(|w| w.eq_ignore_ascii_case(open))?
        + open.len();
    let end = bytes[start..]
        .windows(close.len())
        .position(|w| w.eq_ignore_ascii_case(close))?
        + start;
    let val = xml[start..end].trim();
    if val.is_empty() {
        None
    } else {
        Some(val.replace("&amp;", "&"))
    }
}

/// Extract `(loc, optional lastmod)` pairs from sitemap XML `<url>` blocks.
/// `lastmod` is `None` when the tag is absent — callers should treat absent dates as "recent"
/// (i.e. do not filter out URLs whose age is unknown).
pub fn extract_loc_with_lastmod(xml: &str) -> Vec<(String, Option<String>)> {
    const URL_OPEN: &[u8] = b"<url>";
    const URL_CLOSE: &[u8] = b"</url>";
    let bytes = xml.as_bytes();
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while cursor + URL_OPEN.len() <= bytes.len() {
        let Some(rel) = bytes[cursor..]
            .windows(URL_OPEN.len())
            .position(|w| w.eq_ignore_ascii_case(URL_OPEN))
        else {
            break;
        };
        let block_start = cursor + rel + URL_OPEN.len();
        let block_end = bytes[block_start..]
            .windows(URL_CLOSE.len())
            .position(|w| w.eq_ignore_ascii_case(URL_CLOSE))
            .map(|r| block_start + r)
            .unwrap_or(bytes.len());
        let block = &xml[block_start..block_end];
        if let Some(loc) = extract_between_tags(block, b"<loc>", b"</loc>") {
            let lastmod = extract_between_tags(block, b"<lastmod>", b"</lastmod>");
            out.push((loc, lastmod));
        }
        cursor = block_end + URL_CLOSE.len();
    }
    out
}

pub fn normalize_prefix(prefix: &str) -> Option<String> {
    let trimmed = prefix.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return None;
    }
    let mut value = if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    };
    if value.len() > 1 && value.ends_with('/') {
        value.truncate(value.len() - 1);
    }
    Some(value)
}

pub fn is_excluded_url_path(url: &str, prefixes: &[String]) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let path = parsed.path();
    is_excluded_path(path, prefixes)
        || path
            .trim_start_matches('/')
            .split_once('/')
            .is_some_and(|(_, rest)| is_excluded_path(&format!("/{rest}"), prefixes))
}

fn is_excluded_path(path: &str, prefixes: &[String]) -> bool {
    prefixes.iter().any(|raw| {
        let p = raw.trim().trim_end_matches('/');
        if p.is_empty() || p == "/" {
            return false;
        }
        if p.starts_with('/') {
            return path.eq_ignore_ascii_case(p)
                || (path.len() > p.len()
                    && path[..p.len()].eq_ignore_ascii_case(p)
                    && matches!(
                        path.as_bytes().get(p.len()),
                        Some(&b'/') | Some(&b'-') | None
                    ));
        }
        let implicit_p = format!("/{p}");
        path.eq_ignore_ascii_case(&implicit_p)
            || (path.starts_with('/')
                && path.len() > p.len()
                && path[1..p.len() + 1].eq_ignore_ascii_case(p)
                && matches!(
                    path.as_bytes().get(p.len() + 1),
                    Some(&b'/') | Some(&b'-') | None
                ))
    })
}

pub fn canonicalize_url(url: &str) -> Option<String> {
    let mut parsed = Url::parse(url).ok()?;
    // Strip fragment
    parsed.set_fragment(None);
    // Strip default ports to prevent duplicate entries
    // (http://x:80/p and http://x/p must deduplicate)
    if matches!(
        (parsed.scheme(), parsed.port()),
        ("http", Some(80)) | ("https", Some(443))
    ) {
        let _ = parsed.set_port(None);
    }
    // Strip trailing slashes from all paths (not just root)
    let path = parsed.path().to_string();
    if path.len() > 1 && path.ends_with('/') {
        parsed.set_path(path.trim_end_matches('/'));
    }
    Some(parsed.to_string())
}

pub fn extract_robots_sitemaps(robots_txt: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in robots_txt.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("sitemap") {
            continue;
        }
        let url = value.trim();
        if !url.is_empty() {
            out.push(url.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod exclude_path_tests {
    use super::is_excluded_url_path;

    #[test]
    fn excludes_first_segment_relative_locale_paths() {
        let prefixes = vec!["/fr".to_string(), "/ja".to_string()];
        assert!(is_excluded_url_path(
            "https://example.com/docs/fr/settings",
            &prefixes
        ));
        assert!(is_excluded_url_path(
            "https://example.com/docs/ja-jp/settings",
            &prefixes
        ));
        assert!(!is_excluded_url_path(
            "https://example.com/docs/javascript",
            &prefixes
        ));
    }

    #[test]
    fn excludes_case_insensitive() {
        let prefixes = vec!["/zh-tw".to_string(), "/ZH-CN".to_string()];
        assert!(is_excluded_url_path(
            "https://example.com/zh-TW/mcp",
            &prefixes
        ));
        assert!(is_excluded_url_path(
            "https://example.com/zh-cn/mcp",
            &prefixes
        ));
    }
}
