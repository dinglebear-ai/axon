//! Next.js App Router structured-text scanner (axon_rust-2dhc).
//!
//! Follow-up to xvu9. Next.js App Router (13+) does NOT emit `__NEXT_DATA__`.
//! Instead, the runtime streams React Server Component payloads as multiple
//! `self.__next_f.push([1, "..."])` calls scattered through the HTML.
//!
//! ## What this does (naive)
//! Scans every `<script>` tag for `self.__next_f.push([…])`, pulls the
//! string from each push, then walks the concatenated payload extracting
//! JSON string literals long enough to be content. Filters out class names,
//! paths, asset URLs, hex IDs, and other obvious non-content tokens.
//!
//! ## What this is NOT
//! - A React Flight protocol parser. There's no stable spec, no Rust crate,
//!   and React explicitly does not semver the wire format. Naive scan only.
//! - A typed structured-data source. Output is noisy — class names and ARIA
//!   labels leak through. Acceptable for RAG (embedding model handles
//!   noise) but NOT acceptable for filtering or analysis.
//!
//! ## When to invoke
//! Detection signature: presence of `self.__next_f` in any `<script>` body
//! AND absence of `__NEXT_DATA__` identifies an App Router page. Pages
//! Router pages should continue using `extract_next_data` (xvu9).
//!
//! ## Drift risk
//! The `push()` call shape (`self.__next_f.push([N, "..."])`) has been
//! stable since Next.js 13.4 (App Router stable). The row-level Flight
//! format inside the strings has changed between minor versions but a
//! naive string-leaf scanner is insulated from those changes — it reads
//! quoted leaves, not row markers.

use std::collections::HashSet;

/// Minimum string-literal length to accept as content. Tokens shorter
/// than this are class names, IDs, or short labels.
const MIN_STRING_LEN: usize = 20;

/// Max output strings per page. Hard cap to keep payload bounded.
pub const DEFAULT_MAX_NEXT_APP_STRINGS: usize = 500;

/// Detect: does this HTML look like a Next.js App Router page (uses
/// `self.__next_f.push`, no `__NEXT_DATA__`)?
pub fn is_app_router_page(html: &str) -> bool {
    let lower = html.to_lowercase();
    lower.contains("self.__next_f.push") && !lower.contains("__next_data__")
}

/// Scan all `<script>` tags and return content-bearing string literals
/// pulled from `self.__next_f.push([N, "..."])` calls.
///
/// Returns up to `max_strings` items (default 500). Returns an empty `Vec`
/// when no push calls are present or no content survives the filter.
///
/// Each returned string is unique (dedup by exact match) and passes the
/// content-text heuristic (length, whitespace, non-asset shape).
pub fn extract_next_app_strings(html: &str, max_strings: usize) -> Vec<String> {
    let mut payloads = Vec::new();
    for tag in iter_inline_script_bodies(html) {
        collect_next_f_payloads(&tag, &mut payloads);
    }
    if payloads.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for payload in payloads {
        if out.len() >= max_strings {
            break;
        }
        for s in scan_json_string_literals(&payload) {
            if out.len() >= max_strings {
                break;
            }
            if !is_content_token(&s) {
                continue;
            }
            if !seen.insert(s.clone()) {
                continue;
            }
            out.push(s);
        }
    }
    out
}

// ════════════════════════════════════════════════════════════════════════════
// Script iteration (inline only — no src= attribute)
// ════════════════════════════════════════════════════════════════════════════

/// Iterate inline `<script>` body text from `html`. Skips scripts with
/// `src=` attributes (external) and `type=module` (ES modules). The
/// `self.__next_f.push` calls Next.js emits live in plain inline scripts.
fn iter_inline_script_bodies(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut from = 0;

    while let Some(tag_off) = html[from..].find("<script") {
        let abs = from + tag_off;
        let region = &html[abs..];

        let Some(end_off) = region.find('>') else {
            from = abs + 7;
            continue;
        };
        let opening = &region[..end_off];
        let opening_lower = opening.to_lowercase();

        // Skip external scripts (src=) and module scripts.
        if opening_lower.contains(" src=") || opening_lower.contains("\tsrc=") {
            from = abs + end_off + 1;
            continue;
        }
        if opening_lower.contains("type=\"module\"") || opening_lower.contains("type='module'") {
            from = abs + end_off + 1;
            continue;
        }

        let content_start = abs + end_off + 1;
        let rest = &html[content_start..];
        let Some(close_off) = rest.to_lowercase().find("</script>") else {
            from = content_start;
            continue;
        };

        out.push(rest[..close_off].to_string());
        from = content_start + close_off + 9;
    }

    out
}

// ════════════════════════════════════════════════════════════════════════════
// self.__next_f.push payload extraction
// ════════════════════════════════════════════════════════════════════════════

/// Walk an inline-script body looking for `self.__next_f.push([N, "..."])`
/// calls. Pushes the *string literal* (second array element) onto `out`
/// for each call found.
fn collect_next_f_payloads(script_body: &str, out: &mut Vec<String>) {
    let needle = "self.__next_f.push";
    let mut from = 0;
    while let Some(off) = script_body[from..].find(needle) {
        let abs = from + off + needle.len();
        // Find the opening `(`
        let Some(paren_open) = script_body[abs..].find('(') else {
            from = abs;
            continue;
        };
        let after_paren = abs + paren_open + 1;
        // Skip to the `,` that separates the first and second array
        // element. Format is `[N,"string"]` or `[N, "string"]`.
        let region = &script_body[after_paren..];
        let Some(comma_off) = region.find(',') else {
            from = after_paren;
            continue;
        };
        let after_comma = after_paren + comma_off + 1;
        let region = &script_body[after_comma..];
        // Skip whitespace
        let trimmed = region.trim_start();
        if !trimmed.starts_with('"') {
            from = after_comma;
            continue;
        }
        let quote_abs = after_comma + (region.len() - trimmed.len()) + 1; // past opening quote
        // Read until unescaped closing quote.
        let bytes = script_body.as_bytes();
        let mut i = quote_abs;
        let mut payload = String::new();
        let mut escape = false;
        while i < bytes.len() {
            let c = bytes[i];
            if escape {
                // Decode common escapes; leave the rest as-is.
                match c {
                    b'n' => payload.push('\n'),
                    b'r' => payload.push('\r'),
                    b't' => payload.push('\t'),
                    b'\\' => payload.push('\\'),
                    b'"' => payload.push('"'),
                    b'/' => payload.push('/'),
                    _ => {
                        payload.push('\\');
                        payload.push(c as char);
                    }
                }
                escape = false;
                i += 1;
                continue;
            }
            if c == b'\\' {
                escape = true;
                i += 1;
                continue;
            }
            if c == b'"' {
                break;
            }
            payload.push(c as char);
            i += 1;
        }
        if !payload.is_empty() {
            out.push(payload);
        }
        from = i + 1;
    }
}

// ════════════════════════════════════════════════════════════════════════════
// JSON string-literal scanner
// ════════════════════════════════════════════════════════════════════════════

/// Walk a Flight-protocol payload and emit every `"..."` string literal
/// of length >= `MIN_STRING_LEN`. Handles `\"` escapes; ignores
/// non-string content (numbers, brackets, identifiers).
fn scan_json_string_literals(payload: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = payload.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        // Walk until unescaped closing quote.
        let start = i + 1;
        let mut j = start;
        let mut escape = false;
        let mut decoded = String::new();
        while j < bytes.len() {
            let c = bytes[j];
            if escape {
                match c {
                    b'n' => decoded.push('\n'),
                    b'r' => decoded.push('\r'),
                    b't' => decoded.push('\t'),
                    b'\\' => decoded.push('\\'),
                    b'"' => decoded.push('"'),
                    b'/' => decoded.push('/'),
                    _ => {
                        decoded.push(c as char);
                    }
                }
                escape = false;
                j += 1;
                continue;
            }
            if c == b'\\' {
                escape = true;
                j += 1;
                continue;
            }
            if c == b'"' {
                break;
            }
            decoded.push(c as char);
            j += 1;
        }
        if decoded.len() >= MIN_STRING_LEN {
            out.push(decoded);
        }
        i = j + 1;
    }
    out
}

// ════════════════════════════════════════════════════════════════════════════
// Content-token heuristic
// ════════════════════════════════════════════════════════════════════════════

/// Filter strings that look like content vs. class names, paths, IDs, etc.
///
/// Accept criteria (all must hold):
/// - Length >= MIN_STRING_LEN (already enforced before this is called)
/// - Contains at least one space (multi-word)
/// - Does not start with `/`, `./`, `http://`, `https://`, `data:`,
///   `chunk-`, `webpack-`, `_next/`
/// - Does not look like a file path or asset (doesn't end in known
///   asset extensions)
/// - Letter-density >= 60% (filters base64, hex, IDs)
fn is_content_token(s: &str) -> bool {
    let t = s.trim();
    if t.len() < MIN_STRING_LEN {
        return false;
    }
    if !t.contains(' ') {
        return false;
    }
    if t.starts_with('/') || t.starts_with("./") {
        return false;
    }
    if t.starts_with("http://")
        || t.starts_with("https://")
        || t.starts_with("data:")
        || t.starts_with("chunk-")
        || t.starts_with("webpack-")
        || t.starts_with("_next/")
    {
        return false;
    }
    // Asset extensions
    for ext in &[
        ".woff", ".woff2", ".ttf", ".otf", ".css", ".js", ".mjs", ".svg", ".png", ".jpg", ".webp",
        ".ico", ".json",
    ] {
        if t.ends_with(ext) {
            return false;
        }
    }
    // Letter density
    let letters = t.chars().filter(|c| c.is_alphabetic()).count();
    if (letters as f64) / (t.len() as f64) < 0.60 {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_app_router_page() {
        let html = "<script>self.__next_f.push([1, \"hello world that is long enough text\"])</script>";
        assert!(is_app_router_page(html));
    }

    #[test]
    fn pages_router_page_is_not_app_router() {
        let html = r#"<script id="__NEXT_DATA__" type="application/json">{"props":{}}</script>"#;
        assert!(!is_app_router_page(html));
    }

    #[test]
    fn page_with_neither_marker_is_not_app_router() {
        let html = "<html><body><p>nothing</p></body></html>";
        assert!(!is_app_router_page(html));
    }

    #[test]
    fn extracts_long_string_from_push() {
        // Realistic Flight payload: outer push string contains RSC chunk
        // syntax with inner JSON string literals. Scanner walks the
        // decoded outer string for inner quoted leaves.
        let html = r#"<script>self.__next_f.push([1, "3:\"This is a complete sentence about something useful indeed.\""])</script>"#;
        let out = extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS);
        assert_eq!(out.len(), 1);
        assert!(out[0].contains("This is a complete sentence"));
    }

    #[test]
    fn filters_class_name_short_tokens() {
        // Mixed push: real content + short class-like tokens
        let html = r#"<script>self.__next_f.push([1, "{\"className\":\"mx-auto\",\"text\":\"Welcome to the project documentation page.\"}"])</script>"#;
        let out = extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS);
        // The "mx-auto" class name is too short (8 chars); the prose passes
        assert!(!out.iter().any(|s| s.contains("mx-auto") && s.len() < MIN_STRING_LEN));
        assert!(out.iter().any(|s| s.contains("Welcome to the project")));
    }

    #[test]
    fn filters_url_and_path_tokens() {
        let html = r#"<script>self.__next_f.push([1, "{\"href\":\"/some/nested/path/here/that/is/long\",\"src\":\"https://example.com/img.png\"}"])</script>"#;
        let out = extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS);
        assert!(out.iter().all(|s| !s.starts_with('/') && !s.starts_with("http")));
    }

    #[test]
    fn filters_asset_extensions() {
        let html = r#"<script>self.__next_f.push([1, "static/chunks/main-bundle-abcdef.js"])</script>"#;
        let out = extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS);
        assert!(out.is_empty());
    }

    #[test]
    fn dedupes_repeated_strings() {
        // Same RSC inner-string emitted by two pushes — dedup catches it.
        let html = r#"<script>self.__next_f.push([1, "3:\"This sentence appears more than once on the page.\""])</script>
        <script>self.__next_f.push([1, "4:\"This sentence appears more than once on the page.\""])</script>"#;
        let out = extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn max_strings_cap_respected() {
        let mut html = String::new();
        for i in 0..20 {
            html.push_str(&format!(
                "<script>self.__next_f.push([1, \"{i}:\\\"Unique sentence number {i} long enough to pass filter.\\\"\"])</script>\n"
            ));
        }
        let out = extract_next_app_strings(&html, 3);
        assert!(out.len() <= 3);
    }

    #[test]
    fn skips_external_and_module_scripts() {
        let html = r#"
            <script src="/_next/chunks/main.js"></script>
            <script type="module">import x from "y"; self.__next_f.push([1, "0:\"This should be ignored as a module payload entirely.\""])</script>
            <script>self.__next_f.push([1, "0:\"This is real inline content that the scanner picks up.\""])</script>
        "#;
        let out = extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS);
        assert!(
            out.iter().any(|s| s.contains("real inline content")),
            "scanner must pick up the inline RSC string"
        );
        assert!(
            !out.iter().any(|s| s.contains("ignored as a module")),
            "scanner must skip type=module scripts"
        );
    }

    #[test]
    fn no_push_calls_returns_empty() {
        let html = "<script>console.log('hi');</script>";
        assert!(extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS).is_empty());
    }

    #[test]
    fn handles_escaped_quotes_in_payload() {
        // A push call whose string contains escaped quotes
        let html = r#"<script>self.__next_f.push([1, "He said \"hello world from the project page\" loudly."])</script>"#;
        let out = extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS);
        assert!(out.iter().any(|s| s.contains("hello world from the project")));
    }

    #[test]
    fn content_token_rejects_hex_id() {
        assert!(!is_content_token("a1b2c3d4e5f6a1b2c3d4e5f6"));
    }

    #[test]
    fn content_token_accepts_real_sentence() {
        assert!(is_content_token("This is a complete sentence with letters."));
    }

    #[test]
    fn content_token_rejects_short_string() {
        assert!(!is_content_token("hi there friend"));
    }
}
