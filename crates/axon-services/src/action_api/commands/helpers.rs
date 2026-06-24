use crate::ingest as ingest_svc;
use crate::types::ClientActionError;
use axon_api::mcp_schema::{CrawlRequest, IngestRequest, McpRenderMode, McpScrapeFormat};
use axon_core::config::{Config, RenderMode, ScrapeFormat};

pub(super) fn parse_ingest_source(
    req: &IngestRequest,
    cfg: &Config,
) -> Result<ingest_svc::IngestSource, ClientActionError> {
    ingest_svc::source_from_mcp_request(req, cfg)
        .map_err(|message| ClientActionError::new("invalid_request", message, false, None))
}

pub(super) fn apply_crawl_overrides(cfg: &Config, req: &CrawlRequest) -> Config {
    crate::transport::apply_crawl_overrides(
        cfg,
        &crate::transport::CrawlTransportOverrides {
            max_pages: req.max_pages,
            max_depth: req.max_depth,
            include_subdomains: req.include_subdomains,
            respect_robots: req.respect_robots,
            discover_sitemaps: req.discover_sitemaps,
            max_sitemaps: req.max_sitemaps,
            sitemap_since_days: req.sitemap_since_days,
            discover_llms_txt: req.discover_llms_txt,
            max_llms_txt_urls: req.max_llms_txt_urls,
            render_mode: req.render_mode.map(map_render_mode),
            delay_ms: req.delay_ms,
            collection: None,
            headers: Vec::new(),
        },
    )
}

pub(super) fn map_render_mode(mode: McpRenderMode) -> RenderMode {
    match mode {
        McpRenderMode::Http => RenderMode::Http,
        McpRenderMode::Chrome => RenderMode::Chrome,
        McpRenderMode::AutoSwitch => RenderMode::AutoSwitch,
    }
}

pub(super) fn map_scrape_format(format: McpScrapeFormat) -> ScrapeFormat {
    match format {
        McpScrapeFormat::Markdown => ScrapeFormat::Markdown,
        McpScrapeFormat::Html => ScrapeFormat::Html,
        McpScrapeFormat::RawHtml => ScrapeFormat::RawHtml,
        McpScrapeFormat::Json => ScrapeFormat::Json,
        McpScrapeFormat::Llm => ScrapeFormat::Llm,
    }
}

pub(super) fn parse_viewport(
    raw: Option<&str>,
    fallback_width: u32,
    fallback_height: u32,
) -> Result<(u32, u32), ClientActionError> {
    let Some(raw) = raw else {
        return Ok((fallback_width, fallback_height));
    };
    let Some((width, height)) = raw.split_once('x') else {
        return Err(ClientActionError::new(
            "invalid_request",
            format!("invalid viewport '{raw}': expected WxH"),
            false,
            None,
        ));
    };
    let width = width.parse::<u32>().map_err(|err| {
        ClientActionError::new(
            "invalid_request",
            format!("invalid viewport width '{width}': {err}"),
            false,
            None,
        )
    })?;
    let height = height.parse::<u32>().map_err(|err| {
        ClientActionError::new(
            "invalid_request",
            format!("invalid viewport height '{height}': {err}"),
            false,
            None,
        )
    })?;
    if width == 0 || height == 0 {
        return Err(ClientActionError::new(
            "invalid_request",
            "viewport width and height must be greater than zero",
            false,
            None,
        ));
    }
    Ok((width, height))
}
