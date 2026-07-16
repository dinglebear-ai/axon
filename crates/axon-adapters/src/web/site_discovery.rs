//! In-memory URL discovery for `Map`, `Site`, and `Docs` web scopes.
//!
//! The web engine's map lane owns sitemap, `llms.txt`, and bounded root-anchor
//! URL enumeration. The adapter converts that result directly
//! into canonical manifest items; crawl output directories and
//! `manifest.jsonl` are not part of this contract.

use axon_api::source::*;

use crate::adapter::Result;

use super::manifest_items::web_manifest_item;
use super::options::build_discovery_config;
use super::url_parts::WebUrlParts;

pub(super) struct ManifestDiscovery {
    pub(super) items: Vec<ManifestItem>,
    pub(super) metadata: MetadataMap,
}

pub(super) async fn manifest_items(
    plan: &SourcePlan,
    refresh_content: bool,
) -> Result<ManifestDiscovery> {
    let start_url = plan.route.source.canonical_uri.clone();
    let cfg = build_discovery_config(plan);
    let result = crate::web_engine::engine::discover_site_urls(&cfg, &start_url)
        .await
        .map_err(|err| {
            ApiError::new(
                "adapter.web.discovery_failed",
                axon_error::ErrorStage::Discovering,
                err.to_string(),
            )
        })?;

    let refresh_version = refresh_content
        .then(|| format!("web-discovery:{}:{}", plan.job_id.0, super::timestamp().0));
    let mut urls = result.urls;
    if refresh_content {
        urls.push(start_url);
    }

    let mut items = Vec::with_capacity(urls.len());
    for url in urls {
        let web = WebUrlParts::parse(&url)?;
        let mut item = web_manifest_item(plan, &web, None, None, None);
        item.version = refresh_version.clone();
        items.push(item);
    }
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));
    items.dedup_by(|left, right| left.source_item_key == right.source_item_key);

    if refresh_content && cfg.max_pages > 0 {
        items.truncate(cfg.max_pages as usize);
    }

    let mut metadata = MetadataMap::new();
    metadata.insert(
        "map_source".to_string(),
        serde_json::json!(result.map_source),
    );
    metadata.insert(
        "sitemap_urls".to_string(),
        serde_json::json!(result.sitemap_urls),
    );
    metadata.insert(
        "pages_seen".to_string(),
        serde_json::json!(result.summary.pages_seen),
    );
    metadata.insert(
        "thin_pages".to_string(),
        serde_json::json!(result.summary.thin_pages),
    );
    metadata.insert(
        "elapsed_ms".to_string(),
        serde_json::json!(result.summary.elapsed_ms as u64),
    );
    if let Some(warning) = result.warning {
        metadata.insert("warning".to_string(), serde_json::json!(warning));
    }

    Ok(ManifestDiscovery { items, metadata })
}

#[cfg(test)]
#[path = "site_discovery_tests.rs"]
mod tests;
