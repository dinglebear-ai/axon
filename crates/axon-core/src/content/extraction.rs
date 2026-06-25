use spider::url::Url;
use std::collections::HashSet;

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

/// Extract anchor (`<a href>`) targets, resolving relative hrefs against
/// `base_url`. Unlike [`extract_links`], this resolves relative links to
/// absolute URLs (so a watch's link snapshot is stable run-to-run).
///
/// Scoped to `<a>` opening tags only: a two-pass scan finds each real anchor tag
/// (rejecting `<area`, `<aside`, `<abbr`, …) then reads `href=` within that tag's
/// body. This prevents matching href-like attributes on other elements
/// (`<link href>`, `<base href>`, `<area href>`) that are not navigational links.
pub fn extract_anchor_hrefs(base_url: &str, html: &str, limit: usize) -> Vec<String> {
    let Some(base) = Url::parse(base_url).ok() else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut pos = 0usize;
    let bytes = html.as_bytes();

    // Only extract href= values that appear inside <a ...> tags.
    while pos < html.len() {
        let Some(a_rel) = html[pos..].find("<a") else {
            break;
        };
        let a_start = pos + a_rel;
        let after_a = a_start + 2;

        // The character after "<a" must be whitespace or ">" so we don't match
        // <area, <aside, <abbr, etc.
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

        // Search for href=" or href=' within this tag body only.
        let mut tag_pos = 0usize;
        while let Some(href_rel) = tag_body[tag_pos..].find("href=") {
            let href_at = tag_pos + href_rel;
            let marker = href_at + 5;
            // Require an attribute boundary before `href=` so we don't match
            // `data-href=`, `xhref=`, etc. The byte after `<a` is whitespace, so
            // `href_at` is never 0 for a genuine href attribute.
            if href_at != 0 && !tag_body.as_bytes()[href_at - 1].is_ascii_whitespace() {
                tag_pos = marker;
                continue;
            }
            let Some(quote) = tag_body[marker..].chars().next() else {
                break;
            };
            if quote != '"' && quote != '\'' {
                tag_pos = marker;
                continue;
            }
            let value_start = marker + quote.len_utf8();
            let remain = &tag_body[value_start..];
            let Some(value_end_rel) = remain.find(quote) else {
                break;
            };
            let raw = remain[..value_end_rel].trim();
            tag_pos = value_start + value_end_rel + quote.len_utf8();

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
            if matches!(resolved.scheme(), "http" | "https") {
                let link = resolved.to_string();
                if seen.insert(link.clone()) {
                    out.push(link);
                    if out.len() >= limit {
                        return out;
                    }
                }
            }
        }

        pos = after_a + tag_end_rel + 1;
    }

    out
}
