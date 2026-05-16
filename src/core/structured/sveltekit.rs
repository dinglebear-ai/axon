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
fn js_literal_to_json(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len() + input.len() / 10);
    let mut i = 0;
    let len = bytes.len();

    while i < len {
        let b = bytes[i];

        if b == b'"' {
            out.push('"');
            i += 1;
            while i < len {
                let c = bytes[i];
                out.push(c as char);
                i += 1;
                if c == b'\\' && i < len {
                    out.push(bytes[i] as char);
                    i += 1;
                } else if c == b'"' {
                    break;
                }
            }
            continue;
        }

        if (b == b'{' || b == b',' || b == b'[') && i + 1 < len {
            out.push(b as char);
            i += 1;
            while i < len && bytes[i].is_ascii_whitespace() {
                out.push(bytes[i] as char);
                i += 1;
            }
            if i < len && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
                let key_start = i;
                while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let key = &input[key_start..i];
                while i < len && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                if i < len && bytes[i] == b':' {
                    out.push('"');
                    out.push_str(key);
                    out.push('"');
                } else {
                    out.push_str(key);
                }
            }
            continue;
        }

        out.push(b as char);
        i += 1;
    }

    out
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
