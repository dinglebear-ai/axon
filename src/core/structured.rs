//! Structured-data parallel pass: JSON-LD, `__NEXT_DATA__`, SvelteKit.
//!
//! Runs on every scraped page after the main DOM-to-markdown extraction.
//! Outputs typed payload candidates the embed pipeline can store in Qdrant
//! alongside the markdown chunks.
//!
//! Ports `~/workspace/webclaw/crates/webclaw-core/src/structured_data.rs`
//! (per axon_rust-xvu9) plus `sanitize_json_newlines` (the d5mb sanitizer
//! half — JSON-LD parse fallback for sites that emit raw newlines inside
//! string values, e.g. Bluesky profile pages).
//!
//! Pure parsing — no I/O, WASM-safe.
//!
//! ## Stability notes
//! - **Next.js App Router pages (Next.js 13+)** do NOT emit `__NEXT_DATA__`.
//!   They stream React Server Component payloads via `self.__next_f.push(...)`.
//!   Detect-and-skip is handled here; a follow-up bead (axon_rust-2dhc) ships
//!   a naive `__next_f` string-leaf scanner.
//! - **SvelteKit v2+** may use template literals or BigInt in `kit.start()`
//!   payloads — `js_literal_to_json` does NOT handle either. Test live before
//!   relying.
//! - **JS-literal edge cases not handled**: template literals (backticks),
//!   BigInt literals (`123n`), trailing commas.

use serde_json::Value;

// ════════════════════════════════════════════════════════════════════════════
// Public API
// ════════════════════════════════════════════════════════════════════════════

/// One structured-extraction pass over an HTML page.
///
/// Each field can be empty independently. Pages on Pages Router populate
/// `next_data`; App Router pages do not (use the `__next_f` scanner from
/// the follow-up bead for those).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct StructuredDataPass {
    /// JSON-LD blocks (Schema.org). E-commerce, news, recipes, articles.
    pub json_ld: Vec<Value>,
    /// `__NEXT_DATA__` `props.pageProps` (or the whole envelope as fallback).
    /// Empty `Vec` on App Router pages.
    pub next_data: Vec<Value>,
    /// Parsed payloads from `kit.start(...)` `data:` array, with the
    /// `{type:"data", data:{...}}` wrapper unwrapped where present.
    pub sveltekit: Vec<Value>,
}

impl StructuredDataPass {
    /// True when every output is empty.
    pub fn is_empty(&self) -> bool {
        self.json_ld.is_empty() && self.next_data.is_empty() && self.sveltekit.is_empty()
    }

    /// Total payload count across all three sources.
    pub fn len(&self) -> usize {
        self.json_ld.len() + self.next_data.len() + self.sveltekit.len()
    }
}

/// Run all three extractors against `html` in one pass.
///
/// Each extractor reads the input independently — they don't share state,
/// they can't fail catastrophically, and they all return empty on absent
/// data. Total cost is dominated by the JSON-LD scan + the (usually
/// missing) `__NEXT_DATA__` lookup.
pub fn extract_all(html: &str) -> StructuredDataPass {
    StructuredDataPass {
        json_ld: extract_json_ld(html),
        next_data: extract_next_data(html),
        sveltekit: extract_sveltekit(html),
    }
}

/// Best-effort `@type` extraction from a JSON-LD object.
///
/// Used to populate the indexed `structured_type` keyword field on Qdrant
/// payloads so retrieval can filter by Schema.org type (Article, Product,
/// VideoObject, etc.). Returns the first `@type` string found at the top
/// level. Multi-type entries (`@type: ["Article", "TechArticle"]`) return
/// the first element.
pub fn schema_type_of(value: &Value) -> Option<String> {
    let raw = value.get("@type")?;
    if let Some(s) = raw.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = raw.as_array() {
        return arr.iter().find_map(|v| v.as_str().map(String::from));
    }
    None
}

/// Best-effort `@id` extraction for dedup. Returns `None` if the field is
/// absent or not a string.
pub fn schema_id_of(value: &Value) -> Option<String> {
    value.get("@id").and_then(Value::as_str).map(str::to_string)
}

// ════════════════════════════════════════════════════════════════════════════
// JSON-LD
// ════════════════════════════════════════════════════════════════════════════

/// Extract all JSON-LD blocks from raw HTML.
///
/// Returns parsed JSON values, skipping any block that fails to parse even
/// after newline-sanitization fallback. Most e-commerce sites include
/// Schema.org Product markup with prices, sizes, availability, images.
pub fn extract_json_ld(html: &str) -> Vec<Value> {
    let mut results = Vec::new();
    let needle = "application/ld+json";

    // Walk through the HTML finding `<script type="application/ld+json">`
    // blocks. Simple string scanning — these blocks are self-contained.
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

        // Try parsing as-is; on failure, retry with newline sanitization
        // (Bluesky and similar emit raw newlines inside string values).
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

// ════════════════════════════════════════════════════════════════════════════
// __NEXT_DATA__  (Next.js Pages Router only — App Router uses __next_f)
// ════════════════════════════════════════════════════════════════════════════

/// Extract `__NEXT_DATA__` from Next.js Pages Router pages.
///
/// Returns `props.pageProps` (the actual page data), or the whole envelope
/// as fallback when `pageProps` is missing/empty. Returns an empty vec on
/// App Router pages (which use `self.__next_f.push(...)` instead — see
/// follow-up bead axon_rust-2dhc).
pub fn extract_next_data(html: &str) -> Vec<Value> {
    let Some(id_pos) = html.find("__NEXT_DATA__") else {
        return Vec::new();
    };

    let Some(tag_start) = html[..id_pos].rfind("<script") else {
        return Vec::new();
    };
    let tag_region = &html[tag_start..];

    let Some(tag_end) = tag_region.find('>') else {
        return Vec::new();
    };

    let content_start = tag_start + tag_end + 1;
    let remaining = &html[content_start..];
    let Some(close) = remaining.find("</script>") else {
        return Vec::new();
    };

    let json_str = remaining[..close].trim();
    if json_str.len() < 20 {
        return Vec::new();
    }

    let Ok(data) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };

    if let Some(page_props) = data.get("props").and_then(|p| p.get("pageProps"))
        && page_props.is_object()
        && page_props.as_object().is_some_and(|m| !m.is_empty())
    {
        return vec![page_props.clone()];
    }

    if data.is_object() {
        vec![data]
    } else {
        Vec::new()
    }
}

// ════════════════════════════════════════════════════════════════════════════
// SvelteKit kit.start() data islands
// ════════════════════════════════════════════════════════════════════════════

/// Extract data from SvelteKit's `kit.start()` pattern.
///
/// Locates `kit.start(app, element, { data: [...] })`, balanced-bracket
/// extracts the data array, runs `js_literal_to_json()` to quote unquoted
/// JS object keys, then unwraps `{type:"data", data:{...}}` envelopes.
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
        // SvelteKit wraps as {"type":"data","data":{...}} — unwrap if present
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

// ════════════════════════════════════════════════════════════════════════════
// Helpers (parsing utilities)
// ════════════════════════════════════════════════════════════════════════════

/// Convert a JS object literal to valid JSON by quoting unquoted keys.
///
/// Handles: `{foo:"bar", baz:123}` → `{"foo":"bar", "baz":123}`.
/// Preserves already-quoted keys and string values. Does NOT handle
/// template literals (backticks), BigInt literals (`123n`), or trailing
/// commas — SvelteKit v2+ may emit those.
fn js_literal_to_json(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len() + input.len() / 10);
    let mut i = 0;
    let len = bytes.len();

    while i < len {
        let b = bytes[i];

        // Skip through strings
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

        // After { , [ — look for unquoted identifier followed by `:`
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
                    // bare value like true/false/null
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

/// Replace raw newlines/tabs inside JSON string values with escape sequences.
///
/// Walks the input tracking whether we're inside a quoted string; any
/// literal control character found inside quotes is replaced with its
/// `\n`/`\t`/`\r` escape. Characters outside strings are left untouched.
///
/// This is the d5mb sanitizer half — used as fallback in `extract_json_ld`
/// when the initial `serde_json::from_str` fails (Bluesky-style raw
/// newlines inside JSON-LD string values).
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

// ════════════════════════════════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
#[path = "structured_tests.rs"]
mod tests;
