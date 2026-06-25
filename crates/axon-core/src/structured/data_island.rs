//! JSON data-island walker for thin/SPA pages (axon_rust-1jto).
//!
//! Ports webclaw `webclaw-core/src/data_island.rs` (557 lines). Many modern
//! SPAs (React, Next.js, Nuxt, Contentful) ship server-rendered page data as
//! JSON inside `<script type="application/json">` tags rather than in visible
//! DOM elements. When DOM-based markdown extraction yields sparse output, this
//! walker recovers content from those JSON blobs via five specialized pattern
//! matchers.
//!
//! ## When to invoke
//! Caller checks markdown word count first; only runs the walker when the
//! count is below `sparse_threshold` (default coordinated with axon's
//! `min_markdown_chars` floor). Pages already producing real content skip
//! the walker entirely — this is a recovery fallback, not a general parse.
//!
//! ## What it skips
//! `<script id="__NEXT_DATA__">` is consumed by xvu9's `extract_next_data`;
//! the walker explicitly skips it to avoid duplicate work.
//!
//! ## Five pattern matchers (walked recursively, depth-capped at 15)
//! 1. **Contentful rich-text** — `nodeType: document|paragraph|heading-N|blockquote`
//! 2. **CMS entry** — `{heading|title|headline}` + `{description|subheading|body|text}`
//! 3. **Quote/testimonial** — `{quote|quoteText}` + `{position|author|name}`
//! 4. **Stat array** — `["100M+ users", "#1 rated", ...]`
//! 5. **Orphan body** — `{body|description|subheading|eyebrow|children}`
//!    fields on objects that lack a paired heading
//!
//! ## Decision history (per lavra-research 2026-05-15)
//! 4j1n production data showed thin-page rate = 10.15% over 30 days — above
//! the 5% threshold the bead spec set. The FULL 5-pattern walker ships
//! (Contentful + CMS-entry + quote + stat-array + orphan-body), not just
//! the trimmed Contentful+CMS-entry subset.

use serde_json::{Map, Value};
use std::collections::HashSet;

/// Per-walker max recursion depth on JSON trees. Mirrors webclaw.
const MAX_DEPTH: usize = 15;

/// Maximum chunks emitted per page. Mirrors webclaw. Caller can override
/// via the `max_chunks` argument to `extract_data_islands`.
pub const DEFAULT_MAX_CHUNKS: usize = 1_000;

/// One chunk of text recovered from a JSON data island, with optional heading.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TextChunk {
    heading: Option<String>,
    body: String,
}

// ════════════════════════════════════════════════════════════════════════════
// Public API
// ════════════════════════════════════════════════════════════════════════════

/// Recover markdown from JSON data islands when DOM extraction was sparse.
///
/// `existing_word_count` is the word count of the markdown the DOM walker
/// already produced. When >= `sparse_threshold`, the walker is skipped
/// entirely (returns `None`) — this is recovery, not augmentation.
///
/// `existing_markdown` is used for lowercase-substring deduplication;
/// chunks whose text already appears in the DOM output are dropped.
///
/// `max_chunks` caps the chunk count (default `DEFAULT_MAX_CHUNKS = 1000`).
///
/// Returns `Some(markdown)` when content was recovered, `None` when there
/// was nothing useful (or the page didn't need the walker).
pub fn extract_data_islands(
    html: &str,
    existing_markdown: &str,
    existing_word_count: usize,
    sparse_threshold: usize,
    max_chunks: usize,
) -> Option<String> {
    if existing_word_count >= sparse_threshold {
        return None;
    }

    let mut all_chunks: Vec<TextChunk> = Vec::new();

    for (opening_tag, json_text) in iter_json_script_blocks(html) {
        if all_chunks.len() >= max_chunks {
            break;
        }

        // Skip scripts xvu9 already owns (Next.js Pages Router) to avoid
        // duplicate work. App Router doesn't emit __NEXT_DATA__; it uses
        // self.__next_f which is inline JS, not application/json.
        if opening_tag_has_attr(&opening_tag, "id", "__NEXT_DATA__") {
            continue;
        }

        let trimmed = json_text.trim();
        if trimmed.len() < 50 {
            continue;
        }

        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };

        let mut local: Vec<TextChunk> = Vec::new();
        walk_json(&value, &mut local, 0, max_chunks);
        all_chunks.extend(local);
        all_chunks.truncate(max_chunks);
    }

    if all_chunks.is_empty() {
        return None;
    }

    // Dedup pass: by chunk text, AND against the existing markdown body.
    let existing_lower = existing_markdown.to_lowercase();
    let mut seen: HashSet<String> = HashSet::new();
    all_chunks.retain(|c| {
        let key = if !c.body.is_empty() {
            c.body.clone()
        } else if let Some(h) = &c.heading {
            h.clone()
        } else {
            return false;
        };
        if !seen.insert(key.clone()) {
            return false;
        }
        !existing_lower.contains(&key.to_lowercase())
    });

    if all_chunks.is_empty() {
        return None;
    }

    let mut md = String::new();
    for chunk in &all_chunks {
        if let Some(h) = &chunk.heading {
            md.push_str("\n## ");
            md.push_str(h);
            md.push_str("\n\n");
        }
        if !chunk.body.is_empty() {
            md.push_str(&chunk.body);
            md.push_str("\n\n");
        }
    }
    let md = md.trim().to_string();
    if md.is_empty() { None } else { Some(md) }
}

// ════════════════════════════════════════════════════════════════════════════
// Script-tag iteration (string scanning, no DOM parser)
// ════════════════════════════════════════════════════════════════════════════

/// Iterate `<script type="application/json">...</script>` blocks in `html`.
/// Returns `(opening_tag_inner, body)` pairs where `opening_tag_inner` is
/// the substring between `<script` and `>` (so callers can inspect
/// attributes via `opening_tag_has_attr`).
///
/// Matches the same string-scanning style as the JSON-LD extractor in
/// `json_ld.rs`. No HTML parser, no DOM tree.
fn iter_json_script_blocks(html: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let needle = "application/json";

    let mut from = 0;
    while let Some(tag_off) = html[from..].find("<script") {
        let abs = from + tag_off;
        let region = &html[abs..];

        let Some(end_off) = region.find('>') else {
            from = abs + 7;
            continue;
        };
        let opening = region[..end_off].to_string();

        // Only application/json scripts. Type attribute may be unquoted or
        // case-variant; the substring check is intentionally lax.
        if !opening.to_lowercase().contains(needle) {
            from = abs + end_off + 1;
            continue;
        }

        let content_start = abs + end_off + 1;
        let rest = &html[content_start..];
        let Some(close_off) = rest.to_lowercase().find("</script>") else {
            from = content_start;
            continue;
        };

        let body = rest[..close_off].to_string();
        out.push((opening, body));
        from = content_start + close_off + 9;
    }

    out
}

/// Lax attribute-value check on an opening-tag substring. Returns true if
/// the tag contains an `attr=value` pair (with or without quotes). Used to
/// detect `id="__NEXT_DATA__"` so the walker can skip it.
fn opening_tag_has_attr(opening: &str, attr: &str, value: &str) -> bool {
    let lower = opening.to_lowercase();
    let attr_lower = attr.to_lowercase();
    // Match: attr="value", attr='value', or attr=value (unquoted)
    for prefix in [
        format!("{attr_lower}=\"{value}\""),
        format!("{attr_lower}='{value}'"),
        format!("{attr_lower}={value}"),
    ] {
        if lower.contains(&prefix) {
            return true;
        }
    }
    false
}

// ════════════════════════════════════════════════════════════════════════════
// JSON walker — 5 pattern matchers
// ════════════════════════════════════════════════════════════════════════════

fn walk_json(value: &Value, chunks: &mut Vec<TextChunk>, depth: usize, max_chunks: usize) {
    if depth > MAX_DEPTH || chunks.len() >= max_chunks {
        return;
    }

    match value {
        Value::Object(map) => {
            // Pattern 1: Contentful rich-text node
            if let Some(nt) = map.get("nodeType").and_then(Value::as_str)
                && let Some(chunk) = extract_contentful_node(map, nt)
            {
                chunks.push(chunk);
                return;
            }

            // Pattern 3: Quote/testimonial — checked BEFORE CMS-entry because
            // quotes can have heading+body overlap; quote signal is stronger.
            if let Some(chunk) = extract_quote(map) {
                chunks.push(chunk);
                return;
            }

            // Pattern 2: CMS entry (heading + body fields)
            if is_cms_entry(map)
                && let Some(chunk) = extract_cms_entry(map)
            {
                chunks.push(chunk);
                return;
            }

            // Pattern 5: orphan body strings (extract BEFORE recursing so
            // we don't miss them on objects that also contain nested kids)
            extract_orphan_texts(map, chunks);

            for (key, v) in map {
                if is_media_key(key) {
                    continue;
                }
                walk_json(v, chunks, depth + 1, max_chunks);
            }
        }
        Value::Array(arr) => {
            // Pattern 4: stat array (>=2 content-like strings)
            let content_strings: Vec<&str> = arr
                .iter()
                .filter_map(Value::as_str)
                .filter(|s| s.len() > 10 && s.contains(' '))
                .collect();
            if content_strings.len() >= 2 {
                chunks.push(TextChunk {
                    heading: None,
                    body: content_strings.join(" | "),
                });
                return;
            }
            for v in arr {
                walk_json(v, chunks, depth + 1, max_chunks);
            }
        }
        _ => {}
    }
}

// ── Pattern 1: Contentful rich-text ─────────────────────────────────────────

fn extract_contentful_node(map: &Map<String, Value>, node_type: &str) -> Option<TextChunk> {
    match node_type {
        "document" => {
            let content = map.get("content")?.as_array()?;
            let mut parts: Vec<String> = Vec::new();
            for child in content {
                if let Some(child_map) = child.as_object()
                    && let Some(child_nt) = child_map.get("nodeType").and_then(Value::as_str)
                    && let Some(chunk) = extract_contentful_node(child_map, child_nt)
                {
                    if let Some(h) = &chunk.heading {
                        parts.push(format!("## {h}"));
                    }
                    if !chunk.body.is_empty() {
                        parts.push(chunk.body);
                    }
                }
            }
            if parts.is_empty() {
                return None;
            }
            Some(TextChunk {
                heading: None,
                body: parts.join("\n\n"),
            })
        }
        "paragraph" | "text" => {
            let text = collect_text_content(map);
            if is_content_text(&text) {
                Some(TextChunk {
                    heading: None,
                    body: text,
                })
            } else {
                None
            }
        }
        nt if nt.starts_with("heading-") => {
            let text = collect_text_content(map);
            if text.is_empty() {
                None
            } else {
                Some(TextChunk {
                    heading: Some(text),
                    body: String::new(),
                })
            }
        }
        "blockquote" => {
            let text = collect_text_content(map);
            if is_content_text(&text) {
                Some(TextChunk {
                    heading: None,
                    body: format!("> {text}"),
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn collect_text_content(map: &Map<String, Value>) -> String {
    let mut out = String::new();
    if let Some(v) = map.get("value").and_then(Value::as_str) {
        out.push_str(v);
    }
    if let Some(content) = map.get("content").and_then(Value::as_array) {
        for child in content {
            if let Some(child_map) = child.as_object() {
                let child_text = collect_text_content(child_map);
                out.push_str(&child_text);
            }
        }
    }
    out.trim().to_string()
}

// ── Pattern 2: CMS entry ────────────────────────────────────────────────────

fn is_cms_entry(map: &Map<String, Value>) -> bool {
    let has_heading =
        map.contains_key("heading") || map.contains_key("title") || map.contains_key("headline");
    let has_body = map.contains_key("description")
        || map.contains_key("subheading")
        || map.contains_key("body")
        || map.contains_key("text");
    has_heading && has_body
}

fn extract_cms_entry(map: &Map<String, Value>) -> Option<TextChunk> {
    let heading = extract_text_field(map, "heading")
        .or_else(|| extract_text_field(map, "title"))
        .or_else(|| extract_text_field(map, "headline"))
        .filter(|h| !is_cms_internal_title(h) && h.len() > 5)?;

    let body = extract_text_field(map, "description")
        .or_else(|| extract_text_field(map, "subheading"))
        .or_else(|| extract_text_field(map, "body"))
        .or_else(|| extract_text_field(map, "text"))
        .unwrap_or_default();

    if !is_content_text(&heading) && !is_content_text(&body) {
        return None;
    }

    Some(TextChunk {
        heading: Some(heading),
        body,
    })
}

fn is_cms_internal_title(s: &str) -> bool {
    let t = s.trim();
    t.is_empty()
        || t.starts_with('/')
        || t.contains(':')
        || t.contains(" - ")
        || t.starts_with("Component")
        || t.starts_with("Page ")
}

// ── Pattern 3: Quote/testimonial ────────────────────────────────────────────

fn extract_quote(map: &Map<String, Value>) -> Option<TextChunk> {
    let quote =
        extract_text_field(map, "quote").or_else(|| extract_text_field(map, "quoteText"))?;
    if !is_content_text(&quote) {
        return None;
    }
    let attribution = extract_text_field(map, "position")
        .or_else(|| extract_text_field(map, "author"))
        .or_else(|| extract_text_field(map, "name"))
        .unwrap_or_default();
    let body = if attribution.is_empty() {
        format!("> {quote}")
    } else {
        format!("> {quote}\n> — {attribution}")
    };
    Some(TextChunk {
        heading: None,
        body,
    })
}

// ── Pattern 5: orphan body/heading strings ──────────────────────────────────

fn extract_orphan_texts(map: &Map<String, Value>, chunks: &mut Vec<TextChunk>) {
    const BODY_KEYS: &[&str] = &["body", "description", "subheading", "eyebrow", "children"];
    const HEADING_KEYS: &[&str] = &["heading", "title", "headline"];

    // Don't double-extract entries the CMS path already produced.
    if is_cms_entry(map) {
        return;
    }

    for k in BODY_KEYS {
        if HEADING_KEYS.iter().any(|hk| map.contains_key(*hk)) {
            // Object has BOTH a heading and a body — not orphan; CMS path
            // would have caught it (we returned above) but defensive.
            continue;
        }
        if let Some(text) = extract_text_field(map, k)
            && is_content_text(&text)
        {
            chunks.push(TextChunk {
                heading: None,
                body: text,
            });
            return;
        }
    }

    // Orphan headlines (heading present, no body)
    for k in HEADING_KEYS {
        if BODY_KEYS.iter().any(|bk| map.contains_key(*bk)) {
            continue;
        }
        if let Some(text) = extract_text_field(map, k)
            && is_content_text(&text)
            && !is_cms_internal_title(&text)
        {
            chunks.push(TextChunk {
                heading: Some(text),
                body: String::new(),
            });
            return;
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Extract a text field by name. Handles plain string values AND Contentful
/// rich-text objects (where the value is `{nodeType:"document",content:[...]}`).
fn extract_text_field(map: &Map<String, Value>, key: &str) -> Option<String> {
    let v = map.get(key)?;
    if let Some(s) = v.as_str() {
        let t = s.trim();
        return if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        };
    }
    if let Some(child) = v.as_object()
        && let Some(nt) = child.get("nodeType").and_then(Value::as_str)
        && let Some(chunk) = extract_contentful_node(child, nt)
    {
        let mut text = chunk.heading.unwrap_or_default();
        if !chunk.body.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&chunk.body);
        }
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

/// Heuristic: is this string substantive text vs. an identifier / URL / ID?
fn is_content_text(s: &str) -> bool {
    let t = s.trim();
    if t.len() < 15 {
        return false;
    }
    if !t.contains(' ') {
        return false;
    }
    if t.starts_with("http://") || t.starts_with("https://") || t.starts_with('/') {
        return false;
    }
    let alnum = t.chars().filter(|c| c.is_alphanumeric()).count();
    if alnum == 0 {
        return false;
    }
    (alnum as f64) / (t.len() as f64) >= 0.60
}

/// Skip recursion into media/asset fields — they're URLs, sizes, MIME types,
/// not content we want to surface.
fn is_media_key(key: &str) -> bool {
    matches!(
        key,
        "image"
            | "images"
            | "poster"
            | "video"
            | "videos"
            | "thumbnail"
            | "icon"
            | "logo"
            | "logos"
            | "src"
            | "url"
            | "href"
            | "asset"
            | "assets"
            | "media"
            | "file"
            | "files"
            | "background"
            | "bg"
    )
}

#[cfg(test)]
#[path = "data_island_tests.rs"]
mod tests;
