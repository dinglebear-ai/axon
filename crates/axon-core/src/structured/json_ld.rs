//! JSON-LD `<script type="application/ld+json">` block extraction +
//! newline-sanitization fallback (the d5mb-half sanitizer).

use serde_json::Value;

/// Skip any JSON-LD block whose serialized text exceeds this cap before
/// invoking `serde_json::from_str`. Sized at 8 x the default 64 KiB blob
/// cap so legitimately large product feeds still parse, but a hostile
/// 100 MB block does not pin a worker. Caps the upstream parse cost; the
/// downstream `StructuredPayload::from_pass` cap (default 64 KiB) is the
/// authoritative storage limit.
const MAX_JSON_LD_BLOCK_BYTES: usize = 512 * 1024;

/// Length of the `<script` literal — used as the advance step when an
/// opening tag is malformed and we have no `>` to anchor on.
const SCRIPT_OPEN_PREFIX_LEN: usize = "<script".len();

/// Length of the `</script>` literal — used to advance `search_from` past
/// the closing tag we just consumed.
const SCRIPT_CLOSE_LEN: usize = "</script>".len();

/// Case-insensitive ASCII substring search. Avoids the O(N) allocation of
/// `haystack.to_lowercase()` per call — critical when scanning the whole
/// HTML suffix once per `<script>` tag (O(N x HTML_length) otherwise).
///
/// `needle` MUST be ASCII (callers verify with literals). Returns the
/// index of the first match or `None`.
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

/// Extract all JSON-LD blocks from raw HTML.
///
/// Returns parsed JSON values, skipping any block that fails to parse even
/// after newline-sanitization fallback. Most e-commerce sites include
/// Schema.org Product markup with prices, sizes, availability, images.
pub fn extract_json_ld(html: &str) -> Vec<Value> {
    let mut results = Vec::new();
    let needle = b"application/ld+json";
    let close_tag = b"</script>";
    let html_bytes = html.as_bytes();

    let script_open = b"<script";
    let mut search_from = 0;
    while let Some(tag_start) = ascii_case_insensitive_find(&html_bytes[search_from..], script_open)
    {
        let abs_start = search_from + tag_start;
        let tag_region = &html[abs_start..];

        let Some(tag_end_offset) = tag_region.find('>') else {
            search_from = abs_start + SCRIPT_OPEN_PREFIX_LEN;
            continue;
        };

        let opening_tag_bytes = &html_bytes[abs_start..abs_start + tag_end_offset];

        if ascii_case_insensitive_find(opening_tag_bytes, needle).is_none() {
            search_from = abs_start + tag_end_offset + 1;
            continue;
        }

        let content_start = abs_start + tag_end_offset + 1;
        let Some(close_offset) =
            ascii_case_insensitive_find(&html_bytes[content_start..], close_tag)
        else {
            search_from = content_start;
            continue;
        };

        let json_str = html[content_start..content_start + close_offset].trim();
        search_from = content_start + close_offset + SCRIPT_CLOSE_LEN;

        if json_str.is_empty() {
            continue;
        }

        // Cap parse cost: hostile pages can host multi-MB JSON-LD blocks.
        // Skip pre-parse rather than letting serde_json allocate the full
        // value tree (and sanitize_json_newlines reallocate again).
        if json_str.len() > MAX_JSON_LD_BLOCK_BYTES {
            continue;
        }

        let parsed = serde_json::from_str::<Value>(json_str).or_else(|err| {
            // Only re-attempt with the newline sanitizer when the input
            // actually contains a raw control char the sanitizer fixes.
            // Saves a full string realloc + re-parse on the common case
            // of malformed JSON (trailing commas, unquoted keys, etc.).
            if json_str
                .as_bytes()
                .iter()
                .any(|b| matches!(b, b'\n' | b'\r' | b'\t'))
            {
                let sanitized = sanitize_json_newlines(json_str);
                serde_json::from_str::<Value>(&sanitized)
            } else {
                Err(err)
            }
        });
        match parsed {
            Ok(Value::Array(arr)) => results.extend(arr),
            Ok(val) => results.push(val),
            Err(_) => {}
        }
    }

    results
}

/// Replace raw newlines/tabs inside JSON string values with escape sequences.
///
/// d5mb sanitizer half — used as fallback in `extract_json_ld` when the
/// initial `serde_json::from_str` fails (Bluesky-style raw newlines inside
/// JSON-LD string values).
pub fn sanitize_json_newlines(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escape_next = false;

    for ch in input.chars() {
        if escape_next {
            out.push(ch);
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            out.push(ch);
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            out.push(ch);
            continue;
        }
        if in_string {
            match ch {
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                _ => out.push(ch),
            }
        } else {
            out.push(ch);
        }
    }
    out
}
