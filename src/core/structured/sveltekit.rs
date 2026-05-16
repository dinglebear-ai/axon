//! SvelteKit `kit.start(...)` data island extraction.
//!
//! Targets older SvelteKit `kit.start(app, target, { data: [ ... ] })`
//! shapes. SvelteKit v2+ may use template literals or BigInt in the data
//! payload — `js_literal_to_json` does NOT handle either; test live before
//! relying on extraction for a given site.

use serde_json::Value;

/// Extract data from SvelteKit's `kit.start()` pattern.
pub fn extract_sveltekit(html: &str) -> Vec<Value> {
    let Some(kit_pos) = html.find("kit.start(") else {
        return Vec::new();
    };
    let region = &html[kit_pos..];

    let Some(data_offset) = region.find("data: [") else {
        return Vec::new();
    };
    let bracket_start = kit_pos + data_offset + "data: ".len();
    let bracket_region = &html[bracket_start..];

    let Some(balanced) = extract_balanced(bracket_region, b'[', b']') else {
        return Vec::new();
    };
    if balanced.len() < 50 {
        return Vec::new();
    }

    let json_str = js_literal_to_json(&balanced);
    let Ok(arr) = serde_json::from_str::<Vec<Value>>(&json_str) else {
        return Vec::new();
    };

    let mut results = Vec::new();
    for item in arr {
        if item.is_null() {
            continue;
        }
        if let Some(inner) = item.get("data")
            && (inner.is_object() || inner.is_array())
        {
            results.push(inner.clone());
            continue;
        }
        if item.is_object() || item.is_array() {
            results.push(item);
        }
    }
    results
}

/// Convert a JS object literal to valid JSON by quoting unquoted keys.
///
/// Handles `{foo:"bar", baz:123}` → `{"foo":"bar", "baz":123}`. Does NOT
/// handle template literals, BigInt literals, or trailing commas.
///
/// Accumulates into a `Vec<u8>` and round-trips through `String::from_utf8`
/// so multi-byte UTF-8 inside string literals (non-ASCII characters in
/// SvelteKit page payloads) is preserved verbatim. Falls back to the
/// original input on the impossible case where output is not valid UTF-8
/// (input was already invalid UTF-8 — unreachable for `&str`).
fn js_literal_to_json(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(input.len() + input.len() / 10);
    let mut i = 0;
    let len = bytes.len();

    while i < len {
        let b = bytes[i];

        if b == b'"' {
            out.push(b'"');
            i += 1;
            while i < len {
                let c = bytes[i];
                out.push(c);
                i += 1;
                if c == b'\\' && i < len {
                    out.push(bytes[i]);
                    i += 1;
                } else if c == b'"' {
                    break;
                }
            }
            continue;
        }

        if (b == b'{' || b == b',' || b == b'[') && i + 1 < len {
            out.push(b);
            i += 1;
            while i < len && bytes[i].is_ascii_whitespace() {
                out.push(bytes[i]);
                i += 1;
            }
            if i < len && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
                let key_start = i;
                while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let key = &bytes[key_start..i];
                while i < len && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                if i < len && bytes[i] == b':' {
                    out.push(b'"');
                    out.extend_from_slice(key);
                    out.push(b'"');
                } else {
                    out.extend_from_slice(key);
                }
            }
            continue;
        }

        out.push(b);
        i += 1;
    }

    // `out` is a faithful byte-level rewrite of `input` (a valid `&str`),
    // so the result is always valid UTF-8. Any future change that breaks
    // this invariant (e.g. inserting partial multi-byte sequences) should
    // fail loudly in tests rather than silently fall back to the raw input.
    debug_assert!(
        std::str::from_utf8(&out).is_ok(),
        "js_literal_to_json must preserve UTF-8 byte-for-byte"
    );
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

/// Extract content between balanced brackets, handling string escaping.
fn extract_balanced(text: &str, open: u8, close: u8) -> Option<String> {
    if text.as_bytes().first()? != &open {
        return None;
    }
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, &b) in text.as_bytes().iter().enumerate() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if b == b'\\' && in_string {
            escape_next = true;
            continue;
        }
        if b == b'"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if b == open {
            depth += 1;
        } else if b == close {
            depth -= 1;
            if depth == 0 {
                return Some(text[..=i].to_string());
            }
        }
    }
    None
}
