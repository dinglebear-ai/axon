use axon_core::config::Config;
use axon_crawl::engine::{SitemapDiscovery, discover_sitemap_urls};
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct SitemapDiscoveryStats {
    pub robots_declared_sitemaps: usize,
    pub seeded_default_sitemaps: usize,
    pub discovered_sitemap_documents: usize,
    pub parsed_sitemap_documents: usize,
    pub discovered_urls: usize,
    pub filtered_out_of_scope_host: usize,
    pub filtered_out_of_scope_path: usize,
    pub filtered_excluded_prefix: usize,
    pub failed_fetches: usize,
    pub parse_errors: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct SitemapDiscoveryResult {
    pub urls: Vec<String>,
    pub stats: SitemapDiscoveryStats,
}

impl From<SitemapDiscovery> for SitemapDiscoveryResult {
    fn from(d: SitemapDiscovery) -> Self {
        Self {
            urls: d.urls,
            stats: SitemapDiscoveryStats {
                robots_declared_sitemaps: d.robots_declared_sitemaps,
                seeded_default_sitemaps: d.seeded_default_sitemaps,
                parsed_sitemap_documents: d.parsed_sitemap_documents,
                discovered_urls: d.discovered_urls,
                failed_fetches: d.failed_fetches,
                discovered_sitemap_documents: d.robots_declared_sitemaps
                    + d.seeded_default_sitemaps,
                filtered_out_of_scope_host: 0,
                filtered_out_of_scope_path: 0,
                filtered_excluded_prefix: 0,
                parse_errors: 0,
            },
        }
    }
}

pub(crate) async fn discover_sitemap_urls_with_robots(
    cfg: &Config,
    start_url: &str,
) -> Result<SitemapDiscoveryResult, Box<dyn Error>> {
    let discovery = discover_sitemap_urls(cfg, start_url).await?;
    Ok(SitemapDiscoveryResult::from(discovery))
}
