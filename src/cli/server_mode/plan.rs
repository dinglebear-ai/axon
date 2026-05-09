use crate::cli;
use crate::core::config::{CommandKind, Config};
use crate::mcp::schema::{
    AxonRequest, CrawlRequest, CrawlSubaction, EmbedRequest, EmbedSubaction, ExtractRequest,
    ExtractSubaction, IngestRequest, IngestSourceType, IngestSubaction, McpRenderMode,
    McpScrapeFormat, ResponseMode, ScrapeRequest, ScreenshotRequest, SessionsIngestOptions,
    StatusRequest,
};
use crate::services;
use std::error::Error;
use std::path::Path;

use super::{ServerActionPlan, ServerJobFamily, server_mode_rejects_host_local_embed_input};

pub(super) fn status_action_for_family(family: ServerJobFamily, job_id: &str) -> AxonRequest {
    match family {
        ServerJobFamily::Crawl => AxonRequest::Crawl(crawl_request(
            Some(CrawlSubaction::Status),
            None,
            Some(job_id.to_string()),
        )),
        ServerJobFamily::Extract => AxonRequest::Extract(extract_request(
            Some(ExtractSubaction::Status),
            None,
            None,
            Some(job_id.to_string()),
        )),
        ServerJobFamily::Embed => AxonRequest::Embed(embed_request(
            Some(EmbedSubaction::Status),
            None,
            Some(job_id.to_string()),
        )),
        ServerJobFamily::Ingest => AxonRequest::Ingest(ingest_request(
            Some(IngestSubaction::Status),
            None,
            None,
            Some(job_id.to_string()),
            None,
        )),
    }
}

pub(super) fn server_action_plan(cfg: &Config) -> Result<ServerActionPlan, Box<dyn Error>> {
    match cfg.command {
        CommandKind::Status => Ok(ServerActionPlan {
            action: AxonRequest::Status(StatusRequest {
                subaction: None,
                response_mode: Some(ResponseMode::Inline),
            }),
            label: "status",
            poll_family: None,
        }),
        CommandKind::Scrape => {
            let url = single_url(cfg, "scrape")?;
            Ok(ServerActionPlan {
                action: AxonRequest::Scrape(ScrapeRequest {
                    url: Some(url),
                    render_mode: Some(mcp_render_mode(cfg.render_mode)),
                    format: Some(mcp_scrape_format(cfg.format)),
                    embed: Some(cfg.embed),
                    response_mode: Some(ResponseMode::Inline),
                    root_selector: cfg.root_selector.clone(),
                    exclude_selector: cfg.exclude_selector.clone(),
                }),
                label: "scrape",
                poll_family: None,
            })
        }
        CommandKind::Crawl => crawl_server_action_plan(cfg),
        CommandKind::Extract => extract_server_action_plan(cfg),
        CommandKind::Embed => embed_server_action_plan(cfg),
        CommandKind::Ingest => ingest_server_action_plan(cfg, false),
        CommandKind::Sessions => ingest_server_action_plan(cfg, true),
        CommandKind::Screenshot => {
            let url = single_url(cfg, "screenshot")?;
            Ok(ServerActionPlan {
                action: AxonRequest::Screenshot(ScreenshotRequest {
                    url: Some(url),
                    full_page: Some(cfg.screenshot_full_page),
                    viewport: Some(format!("{}x{}", cfg.viewport_width, cfg.viewport_height)),
                    output: None,
                    response_mode: Some(ResponseMode::Path),
                }),
                label: "screenshot",
                poll_family: None,
            })
        }
        _ => Err(format!("{} is not routed through server mode", cfg.command).into()),
    }
}

fn single_url(cfg: &Config, command: &str) -> Result<String, Box<dyn Error>> {
    let mut urls = cli::commands::common::parse_urls(cfg).into_iter();
    let Some(url) = urls.next() else {
        return Err(format!("{command} requires at least one URL (positional or --urls)").into());
    };
    if urls.next().is_some() {
        return Err(format!("server mode {command} accepts one URL per command for now").into());
    }
    Ok(url)
}

fn crawl_server_action_plan(cfg: &Config) -> Result<ServerActionPlan, Box<dyn Error>> {
    if let Some(subaction) = cfg.positional.first().map(String::as_str) {
        match subaction {
            "status" | "errors" | "cancel" => {
                let id = cfg
                    .positional
                    .get(1)
                    .ok_or_else(|| format!("crawl {subaction} requires <job-id>"))?
                    .to_string();
                let action = match subaction {
                    "cancel" => CrawlSubaction::Cancel,
                    _ => CrawlSubaction::Status,
                };
                return Ok(ServerActionPlan {
                    action: AxonRequest::Crawl(crawl_request(Some(action), None, Some(id))),
                    label: "crawl",
                    poll_family: None,
                });
            }
            "list" | "cleanup" | "clear" | "recover" => {
                let action = match subaction {
                    "list" => CrawlSubaction::List,
                    "cleanup" => CrawlSubaction::Cleanup,
                    "clear" => CrawlSubaction::Clear,
                    "recover" => CrawlSubaction::Recover,
                    _ => unreachable!(),
                };
                return Ok(ServerActionPlan {
                    action: AxonRequest::Crawl(crawl_request(Some(action), None, None)),
                    label: "crawl",
                    poll_family: None,
                });
            }
            "worker" => {
                return Err(
                    "server mode does not start local crawl workers; use `axon serve`".into(),
                );
            }
            "audit" | "diff" => return Err("crawl audit/diff are local-only commands".into()),
            _ => {}
        }
    }

    let urls = cli::commands::common::parse_urls(cfg);
    if urls.is_empty() {
        return Err("crawl requires at least one URL (positional or --urls)".into());
    }
    let mut request = crawl_request(Some(CrawlSubaction::Start), Some(urls), None);
    request.max_pages = Some(cfg.max_pages);
    request.max_depth = Some(cfg.max_depth);
    request.include_subdomains = Some(cfg.include_subdomains);
    request.respect_robots = Some(cfg.respect_robots);
    request.discover_sitemaps = Some(cfg.discover_sitemaps);
    request.sitemap_since_days = Some(cfg.sitemap_since_days);
    request.render_mode = Some(mcp_render_mode(cfg.render_mode));
    request.delay_ms = Some(cfg.delay_ms);
    Ok(ServerActionPlan {
        action: AxonRequest::Crawl(request),
        label: "crawl",
        poll_family: Some(ServerJobFamily::Crawl),
    })
}

fn extract_server_action_plan(cfg: &Config) -> Result<ServerActionPlan, Box<dyn Error>> {
    if let Some(plan) = extract_subcommand_plan(cfg)? {
        return Ok(plan);
    }
    let urls = cli::commands::common::parse_urls(cfg);
    if urls.is_empty() {
        return Err("extract requires at least one URL (positional or --urls)".into());
    }
    let prompt = cfg
        .query
        .clone()
        .ok_or("extract requires --query <prompt>")?;
    let mut request = extract_request(
        Some(ExtractSubaction::Start),
        Some(urls),
        Some(prompt),
        None,
    );
    request.max_pages = Some(cfg.max_pages);
    Ok(ServerActionPlan {
        action: AxonRequest::Extract(request),
        label: "extract",
        poll_family: Some(ServerJobFamily::Extract),
    })
}

pub(super) fn embed_server_action_plan(cfg: &Config) -> Result<ServerActionPlan, Box<dyn Error>> {
    if let Some(plan) = embed_subcommand_plan(cfg)? {
        return Ok(plan);
    }
    let input = cfg.positional.first().cloned().unwrap_or_else(|| {
        cfg.output_dir
            .join("markdown")
            .to_string_lossy()
            .to_string()
    });
    if server_mode_rejects_host_local_embed_input(&input) {
        return Err(
            "server mode does not accept host-local embed paths yet; use a URL/text input or `--local`"
                .into(),
        );
    }
    Ok(ServerActionPlan {
        action: AxonRequest::Embed(embed_request(
            Some(EmbedSubaction::Start),
            Some(input),
            None,
        )),
        label: "embed",
        poll_family: Some(ServerJobFamily::Embed),
    })
}

fn ingest_server_action_plan(
    cfg: &Config,
    sessions: bool,
) -> Result<ServerActionPlan, Box<dyn Error>> {
    if let Some(plan) = ingest_subcommand_plan(cfg, if sessions { "sessions" } else { "ingest" })? {
        return Ok(plan);
    }
    if sessions {
        return Ok(ServerActionPlan {
            action: AxonRequest::Ingest(ingest_request(
                Some(IngestSubaction::Start),
                Some(IngestSourceType::Sessions),
                None,
                None,
                Some(SessionsIngestOptions {
                    claude: Some(cfg.sessions_claude),
                    codex: Some(cfg.sessions_codex),
                    gemini: Some(cfg.sessions_gemini),
                    project: cfg.sessions_project.clone(),
                }),
            )),
            label: "sessions",
            poll_family: Some(ServerJobFamily::Ingest),
        });
    }
    let target = cfg
        .positional
        .first()
        .cloned()
        .ok_or("ingest requires a target")?;
    let source = services::ingest::classify_target(&target, cfg.github_include_source)?;
    let (source_type, target, include_source) = match source {
        services::ingest::IngestSource::Github {
            repo,
            include_source,
        } => (IngestSourceType::Github, repo, Some(include_source)),
        services::ingest::IngestSource::Reddit { target } => {
            (IngestSourceType::Reddit, target, None)
        }
        services::ingest::IngestSource::Youtube { target } => {
            (IngestSourceType::Youtube, target, None)
        }
        services::ingest::IngestSource::Sessions { .. } => {
            return Err("sessions ingest must use the sessions command".into());
        }
    };
    Ok(ServerActionPlan {
        action: AxonRequest::Ingest(IngestRequest {
            subaction: Some(IngestSubaction::Start),
            source_type: Some(source_type),
            target: Some(target),
            include_source,
            sessions: None,
            job_id: None,
            limit: None,
            offset: None,
            response_mode: Some(ResponseMode::Inline),
        }),
        label: "ingest",
        poll_family: Some(ServerJobFamily::Ingest),
    })
}

fn extract_subcommand_plan(cfg: &Config) -> Result<Option<ServerActionPlan>, Box<dyn Error>> {
    let Some(subaction) = cfg.positional.first().map(String::as_str) else {
        return Ok(None);
    };
    let action = match subaction {
        "status" | "errors" => Some(ExtractSubaction::Status),
        "cancel" => Some(ExtractSubaction::Cancel),
        "list" => Some(ExtractSubaction::List),
        "cleanup" => Some(ExtractSubaction::Cleanup),
        "clear" => Some(ExtractSubaction::Clear),
        "recover" => Some(ExtractSubaction::Recover),
        "worker" => {
            return Err(
                "server mode does not start local extract workers; use `axon serve`".into(),
            );
        }
        _ => None,
    };
    let Some(action) = action else {
        return Ok(None);
    };
    let job_id = match action {
        ExtractSubaction::Status | ExtractSubaction::Cancel => Some(
            cfg.positional
                .get(1)
                .ok_or_else(|| format!("extract {subaction} requires <job-id>"))?
                .to_string(),
        ),
        _ => None,
    };
    Ok(Some(ServerActionPlan {
        action: AxonRequest::Extract(extract_request(Some(action), None, None, job_id)),
        label: "extract",
        poll_family: None,
    }))
}

fn embed_subcommand_plan(cfg: &Config) -> Result<Option<ServerActionPlan>, Box<dyn Error>> {
    let Some(subaction) = cfg.positional.first().map(String::as_str) else {
        return Ok(None);
    };
    if cfg.positional.len() == 1 && Path::new(subaction).exists() {
        return Ok(None);
    }
    let action = match subaction {
        "status" | "errors" => Some(EmbedSubaction::Status),
        "cancel" => Some(EmbedSubaction::Cancel),
        "list" => Some(EmbedSubaction::List),
        "cleanup" => Some(EmbedSubaction::Cleanup),
        "clear" => Some(EmbedSubaction::Clear),
        "recover" => Some(EmbedSubaction::Recover),
        "worker" => {
            return Err("server mode does not start local embed workers; use `axon serve`".into());
        }
        _ => None,
    };
    let Some(action) = action else {
        return Ok(None);
    };
    let job_id = match action {
        EmbedSubaction::Status | EmbedSubaction::Cancel => Some(
            cfg.positional
                .get(1)
                .ok_or_else(|| format!("embed {subaction} requires <job-id>"))?
                .to_string(),
        ),
        _ => None,
    };
    Ok(Some(ServerActionPlan {
        action: AxonRequest::Embed(embed_request(Some(action), None, job_id)),
        label: "embed",
        poll_family: None,
    }))
}

fn ingest_subcommand_plan(
    cfg: &Config,
    command_name: &'static str,
) -> Result<Option<ServerActionPlan>, Box<dyn Error>> {
    let Some(subaction) = cfg.positional.first().map(String::as_str) else {
        return Ok(None);
    };
    let action = match subaction {
        "status" | "errors" => Some(IngestSubaction::Status),
        "cancel" => Some(IngestSubaction::Cancel),
        "list" => Some(IngestSubaction::List),
        "cleanup" => Some(IngestSubaction::Cleanup),
        "clear" => Some(IngestSubaction::Clear),
        "recover" => Some(IngestSubaction::Recover),
        "worker" => {
            return Err("server mode does not start local ingest workers; use `axon serve`".into());
        }
        _ => None,
    };
    let Some(action) = action else {
        return Ok(None);
    };
    let job_id = match action {
        IngestSubaction::Status | IngestSubaction::Cancel => Some(
            cfg.positional
                .get(1)
                .ok_or_else(|| format!("{command_name} {subaction} requires <job-id>"))?
                .to_string(),
        ),
        _ => None,
    };
    Ok(Some(ServerActionPlan {
        action: AxonRequest::Ingest(ingest_request(Some(action), None, None, job_id, None)),
        label: command_name,
        poll_family: None,
    }))
}

fn crawl_request(
    subaction: Option<CrawlSubaction>,
    urls: Option<Vec<String>>,
    job_id: Option<String>,
) -> CrawlRequest {
    CrawlRequest {
        subaction,
        urls,
        job_id,
        limit: Some(50),
        offset: Some(0),
        response_mode: Some(ResponseMode::Inline),
        max_pages: None,
        max_depth: None,
        include_subdomains: None,
        respect_robots: None,
        discover_sitemaps: None,
        sitemap_since_days: None,
        render_mode: None,
        delay_ms: None,
    }
}

fn extract_request(
    subaction: Option<ExtractSubaction>,
    urls: Option<Vec<String>>,
    prompt: Option<String>,
    job_id: Option<String>,
) -> ExtractRequest {
    ExtractRequest {
        subaction,
        urls,
        prompt,
        max_pages: None,
        job_id,
        limit: Some(50),
        offset: Some(0),
        response_mode: Some(ResponseMode::Inline),
    }
}

fn embed_request(
    subaction: Option<EmbedSubaction>,
    input: Option<String>,
    job_id: Option<String>,
) -> EmbedRequest {
    EmbedRequest {
        subaction,
        input,
        job_id,
        limit: Some(50),
        offset: Some(0),
        response_mode: Some(ResponseMode::Inline),
    }
}

fn ingest_request(
    subaction: Option<IngestSubaction>,
    source_type: Option<IngestSourceType>,
    target: Option<String>,
    job_id: Option<String>,
    sessions: Option<SessionsIngestOptions>,
) -> IngestRequest {
    IngestRequest {
        subaction,
        source_type,
        target,
        include_source: None,
        sessions,
        job_id,
        limit: Some(50),
        offset: Some(0),
        response_mode: Some(ResponseMode::Inline),
    }
}

fn mcp_render_mode(mode: crate::core::config::RenderMode) -> McpRenderMode {
    match mode {
        crate::core::config::RenderMode::Http => McpRenderMode::Http,
        crate::core::config::RenderMode::Chrome => McpRenderMode::Chrome,
        crate::core::config::RenderMode::AutoSwitch => McpRenderMode::AutoSwitch,
    }
}

fn mcp_scrape_format(format: crate::core::config::ScrapeFormat) -> McpScrapeFormat {
    match format {
        crate::core::config::ScrapeFormat::Markdown => McpScrapeFormat::Markdown,
        crate::core::config::ScrapeFormat::Html => McpScrapeFormat::Html,
        crate::core::config::ScrapeFormat::RawHtml => McpScrapeFormat::RawHtml,
        crate::core::config::ScrapeFormat::Json => McpScrapeFormat::Json,
    }
}
