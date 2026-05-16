//! `__NEXT_DATA__` extraction for Next.js Pages Router pages.
//!
//! Next.js App Router (13+) does not emit `__NEXT_DATA__`; it streams React
//! Server Component payloads via `self.__next_f.push(...)`. App Router
//! pages return an empty `Vec` here — handle that with the follow-up
//! `__next_f` scanner (bd axon_rust-2dhc).

use serde_json::Value;

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
