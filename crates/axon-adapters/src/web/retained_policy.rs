//! URL policy shared by discovery and retained-page refresh.
use super::options;
use axon_api::source::{MetadataMap, SourcePlan};

/// Discovery's explicit blacklist, resolved once for a retained inventory.
pub struct RetainedUrlPolicy {
    excludes: Vec<String>,
    scope_len: usize,
}

impl RetainedUrlPolicy {
    pub fn from_plan(plan: &SourcePlan, discovery_metadata: &MetadataMap) -> Self {
        let cfg = options::build_discovery_config(plan);
        let start = &plan.route.source.canonical_uri;
        let scope_len = match discovery_metadata.get("web_map_scope_prefix") {
            Some(value) => value.as_str().map_or(0, str::len),
            None => crate::web_engine::engine::map::derive_map_scope(start, start)
                .and_then(|scope| scope.path_prefix)
                .map_or(0, |prefix| prefix.len()),
        };
        Self {
            excludes: cfg.exclude_path_prefix,
            scope_len,
        }
    }

    pub fn allows(&self, uri: &str) -> bool {
        !crate::web_engine::engine::map::is_excluded_map_url(uri, &self.excludes, self.scope_len)
    }
}
