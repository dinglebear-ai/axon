//! `__NEXT_DATA__` extraction for Next.js Pages Router pages.
//!
//! Next.js App Router (13+) does not emit `__NEXT_DATA__`; it streams React
//! Server Component payloads via `self.__next_f.push(...)`. App Router
//! pages return an empty `Vec` here — handle that with the follow-up
//! `__next_f` scanner (bd axon_rust-2dhc).

use serde_json::Value;

const SCRIPT_OPEN_PREFIX_LEN: usize = "<script".len();
const SCRIPT_CLOSE_LEN: usize = "</script>".len();

/// ASCII case-insensitive byte search — duplicated from `json_ld.rs` to keep
/// the two modules independent. Both helpers MUST behave identically; if
/// you change one, change the other.
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

/// Return true when the opening-tag byte slice contains a Next.js
/// `id="__NEXT_DATA__"` (or single-quoted) attribute — tolerating
/// arbitrary whitespace and attribute ordering, and matching the
/// `id` attribute name case-insensitively per HTML5.
fn opening_tag_is_next_data(opening: &[u8]) -> bool {
    // Find every `id` attribute occurrence and verify the value is
    // exactly `__NEXT_DATA__`. We can't just substring-search for
    // `__NEXT_DATA__` here because the opener could contain a
    // `data-something="__NEXT_DATA__"` decoy.
    let mut cursor = 0;
    while let Some(pos) = ascii_case_insensitive_find(&opening[cursor..], b"id") {
        let abs = cursor + pos;
        // require id to be a real attribute boundary on the left
        let left_ok = abs == 0
            || matches!(
                opening[abs - 1],
                b' ' | b'\t' | b'\n' | b'\r' | b'\x0C' | b'/'
            );
        let after = abs + 2;
        if !left_ok || after >= opening.len() {
            cursor = abs + 1;
            continue;
        }
        // skip whitespace between `id` and `=`
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
        if &opening[value_start..value_end] == b"__NEXT_DATA__" {
            return true;
        }
        cursor = value_end.max(abs + 1);
    }
    false
}

/// Extract `__NEXT_DATA__` from Next.js Pages Router pages.
///
/// Returns `props.pageProps` (the actual page data), or the whole envelope
/// as fallback when `pageProps` is missing/empty. Returns an empty vec on
/// App Router pages (which use `self.__next_f.push(...)` instead — see
/// follow-up bead axon_rust-2dhc).
///
/// cubic finding #4 (next_data.rs:17): the previous implementation searched
/// for the first `"__NEXT_DATA__"` substring anywhere in the document and
/// derived the script boundary from that — comments, inline references, or
/// `data-*="__NEXT_DATA__"` attributes earlier in the page silently masked
/// the real script. The fix walks `<script ...>` tags in order and only
/// accepts one whose opening tag carries `id="__NEXT_DATA__"`.
pub fn extract_next_data(html: &str) -> Vec<Value> {
    let html_bytes = html.as_bytes();
    let script_open = b"<script";
    let script_close = b"</script>";
    let mut search_from = 0;

    while let Some(tag_start) = ascii_case_insensitive_find(&html_bytes[search_from..], script_open)
    {
        let abs_start = search_from + tag_start;
        let tag_region = &html[abs_start..];

        let Some(tag_end_offset) = tag_region.find('>') else {
            search_from = abs_start + SCRIPT_OPEN_PREFIX_LEN;
            continue;
        };

        let opening_tag = &html_bytes[abs_start..abs_start + tag_end_offset];
        if !opening_tag_is_next_data(opening_tag) {
            search_from = abs_start + tag_end_offset + 1;
            continue;
        }

        let content_start = abs_start + tag_end_offset + 1;
        let Some(close_offset) =
            ascii_case_insensitive_find(&html_bytes[content_start..], script_close)
        else {
            return Vec::new();
        };

        let json_str = html[content_start..content_start + close_offset].trim();
        if json_str.len() < 20 {
            search_from = content_start + close_offset + SCRIPT_CLOSE_LEN;
            continue;
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
            return vec![data];
        }
        return Vec::new();
    }

    Vec::new()
}
