//! JSON-LD `<script type="application/ld+json">` block extraction +
//! newline-sanitization fallback (the d5mb-half sanitizer).

use serde_json::Value;

/// Extract all JSON-LD blocks from raw HTML.
///
/// Returns parsed JSON values, skipping any block that fails to parse even
/// after newline-sanitization fallback. Most e-commerce sites include
/// Schema.org Product markup with prices, sizes, availability, images.
pub fn extract_json_ld(html: &str) -> Vec<Value> {
    let mut results = Vec::new();
    let needle = "application/ld+json";

    let mut search_from = 0;
    while let Some(tag_start) = html[search_from..].find("<script") {
        let abs_start = search_from + tag_start;
        let tag_region = &html[abs_start..];

        let Some(tag_end_offset) = tag_region.find('>') else {
            search_from = abs_start + 7;
            continue;
        };

        let opening_tag = &tag_region[..tag_end_offset];

        if !opening_tag.to_lowercase().contains(needle) {
            search_from = abs_start + tag_end_offset + 1;
            continue;
        }

        let content_start = abs_start + tag_end_offset + 1;
        let remaining = &html[content_start..];
        let Some(close_offset) = remaining.to_lowercase().find("</script>") else {
            search_from = content_start;
            continue;
        };

        let json_str = remaining[..close_offset].trim();
        search_from = content_start + close_offset + 9;

        if json_str.is_empty() {
            continue;
        }

        let parsed = serde_json::from_str::<Value>(json_str).or_else(|_| {
            let sanitized = sanitize_json_newlines(json_str);
            serde_json::from_str::<Value>(&sanitized)
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
