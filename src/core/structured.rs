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

mod data_island;
mod json_ld;
mod next_app;
mod next_data;
mod sveltekit;
#[cfg(test)]
mod tests;

pub use data_island::{DEFAULT_MAX_CHUNKS as DATA_ISLAND_DEFAULT_MAX_CHUNKS, extract_data_islands};
pub use json_ld::{extract_json_ld, sanitize_json_newlines};
pub use next_app::{
    DEFAULT_MAX_NEXT_APP_STRINGS, extract_next_app_strings, is_app_router_page,
};
pub use next_data::extract_next_data;
pub use sveltekit::extract_sveltekit;

use serde_json::Value;

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

    /// Return the dominant (kind, value) pair to attach to a doc's Qdrant
    /// payload, preferring richer structured sources over generic ones:
    /// JSON-LD > `__NEXT_DATA__` > SvelteKit.
    ///
    /// Returns `None` when the pass is empty.
    pub fn dominant(&self) -> Option<(&'static str, &Value)> {
        if let Some(v) = self.json_ld.first() {
            return Some(("jsonld", v));
        }
        if let Some(v) = self.next_data.first() {
            return Some(("next_data", v));
        }
        if let Some(v) = self.sveltekit.first() {
            return Some(("sveltekit", v));
        }
        None
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
