use crate::core::config::{Config, ConfigOverrides, RenderMode, ScrapeFormat};
use crate::mcp::schema::{
    CrawlRequest, IngestRequest, IngestSourceType, McpRenderMode, McpScrapeFormat,
};
use crate::services::ingest as ingest_svc;
use crate::services::types::ClientActionError;

pub(super) fn parse_ingest_source(
    req: &IngestRequest,
    cfg: &Config,
) -> Result<ingest_svc::IngestSource, ClientActionError> {
    let source_type = req.source_type.clone().ok_or_else(|| {
        ClientActionError::new(
            "invalid_request",
            "source_type is required for ingest.start",
            false,
            None,
        )
    })?;
    match source_type {
        IngestSourceType::Github => {
            let repo = req.target.clone().ok_or_else(|| {
                ClientActionError::new("invalid_request", "target repo is required", false, None)
            })?;
            Ok(ingest_svc::IngestSource::Github {
                repo,
                include_source: req.include_source.unwrap_or(cfg.github_include_source),
            })
        }
        IngestSourceType::Reddit => {
            let target = req.target.clone().ok_or_else(|| {
                ClientActionError::new("invalid_request", "target is required", false, None)
            })?;
            Ok(ingest_svc::IngestSource::Reddit { target })
        }
        IngestSourceType::Youtube => {
            let target = req.target.clone().ok_or_else(|| {
                ClientActionError::new("invalid_request", "target is required", false, None)
            })?;
            Ok(ingest_svc::IngestSource::Youtube { target })
        }
        IngestSourceType::Sessions => {
            let sessions =
                req.sessions
                    .clone()
                    .unwrap_or(crate::mcp::schema::SessionsIngestOptions {
                        claude: None,
                        codex: None,
                        gemini: None,
                        project: None,
                    });
            Ok(ingest_svc::IngestSource::Sessions {
                sessions_claude: sessions.claude.unwrap_or(false),
                sessions_codex: sessions.codex.unwrap_or(false),
                sessions_gemini: sessions.gemini.unwrap_or(false),
                sessions_project: sessions.project,
            })
        }
    }
}

pub(super) fn apply_crawl_overrides(cfg: &Config, req: &CrawlRequest) -> Config {
    cfg.apply_overrides(&ConfigOverrides {
        max_pages: req.max_pages,
        max_depth: req.max_depth,
        include_subdomains: req.include_subdomains,
        respect_robots: req.respect_robots,
        discover_sitemaps: req.discover_sitemaps,
        sitemap_since_days: req.sitemap_since_days,
        render_mode: req.render_mode.map(map_render_mode),
        delay_ms: req.delay_ms,
        ..ConfigOverrides::default()
    })
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

pub(super) fn internal_message(message: String) -> ClientActionError {
    ClientActionError::new("internal", message, true, None)
}
