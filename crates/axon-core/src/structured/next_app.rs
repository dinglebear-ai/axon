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

/// ASCII case-insensitive byte search. Avoids the full-document
/// `html.to_lowercase()` allocation that the previous implementation paid
/// per call (and per `<script>` tag in the iterator below).
fn ascii_case_insensitive_find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    let last = haystack.len() - needle.len();
    'outer: for i in 0..=last {
        for (j, &n) in needle.iter().enumerate() {
            if !haystack[i + j].eq_ignore_ascii_case(&n) {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}

/// Detect: does this HTML look like a Next.js App Router page (uses
/// `self.__next_f.push`, no `__NEXT_DATA__`)?
///
/// Token search is ASCII-only and case-insensitive — avoids the
/// `html.to_lowercase()` allocation on multi-MB pages.
pub fn is_app_router_page(html: &str) -> bool {
    let bytes = html.as_bytes();
    ascii_case_insensitive_find(bytes, b"self.__next_f.push").is_some()
        && ascii_case_insensitive_find(bytes, b"__next_data__").is_none()
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

/// True when `opening` (the bytes between `<script` and `>`, exclusive of
/// both) carries an attribute named `attr` at a real attribute boundary.
///
/// "Boundary" means the byte before the attribute name is one of the HTML5
/// whitespace characters (space, tab, newline, CR, form feed) — i.e. an
/// attribute separator — not part of another attribute name like
/// `data-src` or `nosrc`.
fn opening_tag_has_attribute(opening: &[u8], attr: &[u8]) -> bool {
    let mut cursor = 0;
    while let Some(pos) = ascii_case_insensitive_find(&opening[cursor..], attr) {
        let abs = cursor + pos;
        let left_ok = abs == 0
            || matches!(
                opening[abs - 1],
                b' ' | b'\t' | b'\n' | b'\r' | b'\x0C' | b'/'
            );
        let after = abs + attr.len();
        // Right side must be `=`, whitespace, `/`, or end of tag — otherwise
        // we've matched a longer attribute name (e.g. `srcset` for `src`).
        let right_ok = after >= opening.len()
            || matches!(
                opening[after],
                b'=' | b' ' | b'\t' | b'\n' | b'\r' | b'\x0C' | b'/'
            );
        if left_ok && right_ok {
            return true;
        }
        cursor = abs + 1;
    }
    false
}

/// True when the opening tag has `type="module"` (any quoting) or unquoted
/// `type=module`. Only matches at a real attribute boundary.
fn opening_tag_is_module_script(opening: &[u8]) -> bool {
    let mut cursor = 0;
    while let Some(pos) = ascii_case_insensitive_find(&opening[cursor..], b"type") {
        let abs = cursor + pos;
        let left_ok = abs == 0
            || matches!(
                opening[abs - 1],
                b' ' | b'\t' | b'\n' | b'\r' | b'\x0C' | b'/'
            );
        let after = abs + 4;
        if !left_ok || after >= opening.len() {
            cursor = abs + 1;
            continue;
        }
        // skip whitespace + `=`
        let mut i = after;
        while i < opening.len() && matches!(opening[i], b' ' | b'\t' | b'\n' | b'\r' | b'\x0C') {
            i += 1;
        }
        if i >= opening.len() || opening[i] != b'=' {
            cursor = abs + 1;
            continue;
        }
        i += 1;
        while i < opening.len() && matches!(opening[i], b' ' | b'\t' | b'\n' | b'\r' | b'\x0C') {
            i += 1;
        }
        let quote = if i < opening.len() && (opening[i] == b'"' || opening[i] == b'\'') {
            let q = opening[i];
            i += 1;
            Some(q)
        } else {
            None
        };
        let value_start = i;
        let value_end = match quote {
            Some(q) => {
                while i < opening.len() && opening[i] != q {
                    i += 1;
                }
                i
            }
            None => {
                while i < opening.len()
                    && !matches!(opening[i], b' ' | b'\t' | b'\n' | b'\r' | b'\x0C' | b'/')
                {
                    i += 1;
                }
                i
            }
        };
        if opening[value_start..value_end].eq_ignore_ascii_case(b"module") {
            return true;
        }
        cursor = value_end.max(abs + 1);
    }
    false
}

/// Iterate inline `<script>` body text from `html`. Skips scripts with
/// `src=` attributes (external) and `type=module` (ES modules). The
/// `self.__next_f.push` calls Next.js emits live in plain inline scripts.
///
/// All scans are byte-level and case-insensitive — we never allocate a
/// full lowercase copy of the page (the previous implementation cloned
/// `html[content_start..]` per `<script>` tag, which was O(N x HTML_len)).
fn iter_inline_script_bodies(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let html_bytes = html.as_bytes();
    let mut from = 0;

    while let Some(tag_off) = ascii_case_insensitive_find(&html_bytes[from..], b"<script") {
        let abs = from + tag_off;

        let Some(end_off) = html[abs..].find('>') else {
            from = abs + 7;
            continue;
        };
        // Opening-tag bytes between `<script` (inclusive) and `>` (exclusive).
        // Skip past the leading `<script` so `opening_tag_has_attribute` sees
        // the attribute region starting at the byte after the tag name.
        let opening = &html_bytes[abs + 7..abs + end_off];

        // Skip external scripts (src=) and module scripts.
        if opening_tag_has_attribute(opening, b"src") || opening_tag_is_module_script(opening) {
            from = abs + end_off + 1;
            continue;
        }

        let content_start = abs + end_off + 1;
        let Some(close_off) =
            ascii_case_insensitive_find(&html_bytes[content_start..], b"</script>")
        else {
            from = content_start;
            continue;
        };

        out.push(html[content_start..content_start + close_off].to_string());
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
    // Asset extensions — by this point `t` is guaranteed to contain a
    // space (multi-word check above), so any token still ending in an
    // asset extension here looks like asset metadata leaking into prose
    // (e.g. "background: hero.png"); reject it.
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
#[path = "next_app_tests.rs"]
mod tests;
