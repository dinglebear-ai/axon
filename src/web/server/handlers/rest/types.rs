//! Shared request body types for REST handlers.
//!
//! Each per-resource POST route has its own body struct so callers do not
//! need to embed an `action` discriminator. Bodies deserialize directly into
//! the inputs accepted by `services::*` entry points.

use serde::Deserialize;

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueryBody {
    pub query: String,
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetrieveBody {
    pub url: String,
    #[serde(default)]
    pub max_points: Option<usize>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub token_budget: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct SuggestBody {
    #[serde(default)]
    pub focus: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct UrlOnlyBody {
    pub url: String,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct UrlsBody {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub urls: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct MapBody {
    pub url: String,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchBody {
    pub query: String,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
    /// Accepts: "day" | "week" | "month" | "year"
    #[serde(default)]
    pub time_range: Option<String>,
}

// ── Family 3 — async job submission bodies ────────────────────────────────

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct CrawlSubmitBody {
    pub urls: Vec<String>,
    #[serde(default)]
    pub max_pages: Option<u32>,
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub render_mode: Option<crate::core::config::RenderMode>,
    #[serde(default)]
    pub include_subdomains: Option<bool>,
    #[serde(default)]
    pub respect_robots: Option<bool>,
    #[serde(default)]
    pub discover_sitemaps: Option<bool>,
    #[serde(default)]
    pub max_sitemaps: Option<usize>,
    #[serde(default)]
    pub sitemap_since_days: Option<u32>,
    #[serde(default)]
    pub delay_ms: Option<u64>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmbedSubmitBody {
    pub input: String,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub collection: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExtractSubmitBody {
    pub urls: Vec<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub extract_mode: Option<crate::services::client_contract::ClientExtractMode>,
    #[serde(default)]
    pub max_pages: Option<u32>,
    #[serde(default)]
    pub render_mode: Option<crate::core::config::RenderMode>,
    #[serde(default)]
    pub embed: Option<bool>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
}
