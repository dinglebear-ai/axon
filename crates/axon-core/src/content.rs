mod deterministic;
mod endpoints;
mod engine;
mod extract_ladder;
mod extraction;
mod filename;
pub mod llm;
pub(crate) mod markdown;
mod url_parsing;

#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;

pub use deterministic::{
    DeterministicExtractionEngine, DeterministicParser, ExtractRun, ExtractionMetrics,
    PageExtraction,
};
pub use endpoints::{
    DEFAULT_MAX_ENDPOINTS, DEFAULT_MAX_SCAN_BYTES, DEFAULT_MAX_SCRIPTS, DiscoveredEndpoint,
    EndpointExtractOptions, EndpointKind, EndpointOptions, EndpointReport, EndpointSourceKind,
    EndpointVerification, McpCandidateAttempt, McpHostKind, McpProbeOutcome, PrefetchedBundle,
    RpcProbeResult, RpcProtocol, RpcTransport, ScriptSource, discover_script_sources,
    endpoint_host_counts, extract_endpoints,
};
pub use engine::{ExtractWebConfig, run_extract_with_engine};
pub use extract_ladder::{LadderResult, LadderThresholds, LadderTier, extract_with_ladder};
pub use extraction::{extract_anchor_hrefs, extract_links, extract_meta_description, find_between};
pub use filename::{url_to_domain, url_to_filename, url_to_stable_filename};
pub use llm::to_llm_text;
pub use markdown::{
    BOILERPLATE_SELECTORS, build_selector_config, build_transform_config, bytes_to_markdown,
    clean_markdown_whitespace, redact_url, to_markdown,
};
pub use url_parsing::{
    canonicalize_url, extract_loc_values, extract_loc_with_lastmod, extract_robots_sitemaps,
    is_excluded_url_path, normalize_prefix,
};
