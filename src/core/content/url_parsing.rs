use spider::url::Url;

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
#[path = "url_parsing_tests.rs"]
mod tests;
