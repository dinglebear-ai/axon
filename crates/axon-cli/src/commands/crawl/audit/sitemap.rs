use axon_core::config::Config;
use axon_services::crawl as crawl_service;
use std::error::Error;

pub(crate) use crawl_service::{SitemapDiscoveryResult, SitemapDiscoveryStats};

pub(crate) async fn discover_sitemap_urls_with_robots(
    cfg: &Config,
    start_url: &str,
) -> Result<SitemapDiscoveryResult, Box<dyn Error>> {
    crawl_service::discover_sitemap_urls_with_robots(cfg, start_url).await
}
