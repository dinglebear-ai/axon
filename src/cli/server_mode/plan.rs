use crate::cli;
use crate::core::config::{CommandKind, Config};
use std::error::Error;

use super::{ServerJobFamily, server_mode_rejects_host_local_embed_input};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ServerRestPlan {
    pub method: &'static str,
    pub path: String,
    pub body: serde_json::Value,
    pub label: &'static str,
    pub poll_family: Option<ServerJobFamily>,
}

pub(crate) fn status_path_for_family(family: ServerJobFamily, job_id: &str) -> String {
    match family {
        ServerJobFamily::Crawl => format!("/v1/crawl/{job_id}"),
        ServerJobFamily::Extract => format!("/v1/extract/{job_id}"),
        ServerJobFamily::Embed => format!("/v1/embed/{job_id}"),
        ServerJobFamily::Ingest => format!("/v1/ingest/{job_id}"),
    }
}

pub(crate) fn server_rest_plan(cfg: &Config) -> Result<ServerRestPlan, Box<dyn Error>> {
    match cfg.command {
        CommandKind::Status => Ok(ServerRestPlan {
            method: "GET",
            path: "/v1/status".to_string(),
            body: serde_json::Value::Null,
            label: "status",
            poll_family: None,
        }),
        CommandKind::Scrape => {
            let url = single_url(cfg, "scrape")?;
            Ok(ServerRestPlan {
                method: "POST",
                path: "/v1/scrape".to_string(),
                body: serde_json::json!({ "url": url }),
                label: "scrape",
                poll_family: None,
            })
        }
        CommandKind::Summarize => {
            let urls = cli::commands::common::parse_urls(cfg);
            if urls.is_empty() {
                return Err("summarize requires at least one URL (positional or --urls)".into());
            }
            Ok(ServerRestPlan {
                method: "POST",
                path: "/v1/summarize".to_string(),
                body: serde_json::json!({ "urls": urls }),
                label: "summarize",
                poll_family: None,
            })
        }
        CommandKind::Crawl => crawl_server_rest_plan(cfg),
        CommandKind::Extract => extract_server_rest_plan(cfg),
        CommandKind::Embed => embed_server_rest_plan(cfg),
        CommandKind::Ingest => ingest_server_rest_plan(cfg, false),
        CommandKind::Sessions => ingest_server_rest_plan(cfg, true),
        _ => Err(format!("{} is not routed through server mode", cfg.command).into()),
    }
}

fn crawl_server_rest_plan(cfg: &Config) -> Result<ServerRestPlan, Box<dyn Error>> {
    if let Some(subaction) = cfg.positional.first().map(String::as_str)
        && let Some(plan) =
            async_job_lifecycle_plan("crawl", ServerJobFamily::Crawl, subaction, cfg)?
    {
        return Ok(plan);
    }
    let urls = cli::commands::common::parse_urls(cfg);
    if urls.is_empty() {
        return Err("crawl requires at least one URL (positional or --urls)".into());
    }
    Ok(ServerRestPlan {
        method: "POST",
        path: "/v1/crawl".to_string(),
        body: serde_json::json!({
            "urls": urls,
            "max_pages": cfg.max_pages,
            "max_depth": cfg.max_depth,
            "render_mode": cfg.render_mode,
            "include_subdomains": cfg.include_subdomains,
            "respect_robots": cfg.respect_robots,
            "discover_sitemaps": cfg.discover_sitemaps,
            "max_sitemaps": cfg.max_sitemaps,
            "sitemap_since_days": cfg.sitemap_since_days,
            "delay_ms": cfg.delay_ms,
        }),
        label: "crawl",
        poll_family: Some(ServerJobFamily::Crawl),
    })
}

fn extract_server_rest_plan(cfg: &Config) -> Result<ServerRestPlan, Box<dyn Error>> {
    if let Some(subaction) = cfg.positional.first().map(String::as_str)
        && let Some(plan) =
            async_job_lifecycle_plan("extract", ServerJobFamily::Extract, subaction, cfg)?
    {
        return Ok(plan);
    }
    let urls = cli::commands::common::parse_urls(cfg);
    if urls.is_empty() {
        return Err("extract requires at least one URL (positional or --urls)".into());
    }
    Ok(ServerRestPlan {
        method: "POST",
        path: "/v1/extract".to_string(),
        body: serde_json::json!({
            "urls": urls,
            "prompt": cfg.query,
            "max_pages": cfg.max_pages,
            "render_mode": cfg.render_mode,
            "embed": cfg.embed,
        }),
        label: "extract",
        poll_family: Some(ServerJobFamily::Extract),
    })
}

fn embed_server_rest_plan(cfg: &Config) -> Result<ServerRestPlan, Box<dyn Error>> {
    if let Some(subaction) = cfg.positional.first().map(String::as_str)
        && let Some(plan) =
            async_job_lifecycle_plan("embed", ServerJobFamily::Embed, subaction, cfg)?
    {
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
    Ok(ServerRestPlan {
        method: "POST",
        path: "/v1/embed".to_string(),
        body: serde_json::json!({
            "input": input,
            "collection": cfg.collection,
        }),
        label: "embed",
        poll_family: Some(ServerJobFamily::Embed),
    })
}

fn ingest_server_rest_plan(cfg: &Config, sessions: bool) -> Result<ServerRestPlan, Box<dyn Error>> {
    if !sessions
        && let Some(subaction) = cfg.positional.first().map(String::as_str)
        && let Some(plan) =
            async_job_lifecycle_plan("ingest", ServerJobFamily::Ingest, subaction, cfg)?
    {
        return Ok(plan);
    }
    if sessions {
        return Ok(ServerRestPlan {
            method: "POST",
            path: "/v1/ingest".to_string(),
            body: serde_json::json!({
                "source_type": "sessions",
                "sessions_claude": cfg.sessions_claude,
                "sessions_codex": cfg.sessions_codex,
                "sessions_gemini": cfg.sessions_gemini,
                "sessions_project": cfg.sessions_project,
            }),
            label: "sessions",
            poll_family: Some(ServerJobFamily::Ingest),
        });
    }
    let target = cfg.positional.first().ok_or("ingest requires <target>")?;
    let source = crate::services::ingest::classify_target(target, cfg.github_include_source)?;
    Ok(ServerRestPlan {
        method: "POST",
        path: "/v1/ingest".to_string(),
        body: serde_json::to_value(source)?,
        label: "ingest",
        poll_family: Some(ServerJobFamily::Ingest),
    })
}

fn single_url(cfg: &Config, command: &str) -> Result<String, Box<dyn Error>> {
    let urls = cli::commands::common::parse_urls(cfg);
    match urls.as_slice() {
        [] => Err(format!("{command} requires a URL").into()),
        [url] => Ok(url.clone()),
        _ => Err(format!("{command} accepts exactly one URL in server mode").into()),
    }
}

fn async_job_lifecycle_plan(
    family: &'static str,
    poll_family: ServerJobFamily,
    subaction: &str,
    cfg: &Config,
) -> Result<Option<ServerRestPlan>, Box<dyn Error>> {
    let plan = match subaction {
        "status" | "errors" => {
            let id = cfg
                .positional
                .get(1)
                .ok_or_else(|| format!("{family} {subaction} requires <job-id>"))?;
            ServerRestPlan {
                method: "GET",
                path: status_path_for_family(poll_family, id),
                body: serde_json::Value::Null,
                label: family,
                poll_family: None,
            }
        }
        "list" => ServerRestPlan {
            method: "GET",
            path: format!("/v1/{family}"),
            body: serde_json::Value::Null,
            label: family,
            poll_family: None,
        },
        "cleanup" | "recover" => ServerRestPlan {
            method: "POST",
            path: format!("/v1/{family}/{subaction}"),
            body: serde_json::json!({}),
            label: family,
            poll_family: None,
        },
        "clear" => ServerRestPlan {
            method: "DELETE",
            path: format!("/v1/{family}"),
            body: serde_json::Value::Null,
            label: family,
            poll_family: None,
        },
        "cancel" => {
            let id = cfg
                .positional
                .get(1)
                .ok_or_else(|| format!("{family} cancel requires <job-id>"))?;
            ServerRestPlan {
                method: "POST",
                path: format!("/v1/{family}/{id}/cancel"),
                body: serde_json::json!({}),
                label: family,
                poll_family: None,
            }
        }
        "worker" => {
            return Err(format!(
                "server mode does not start local {family} workers; use `axon serve`"
            )
            .into());
        }
        _ => return Ok(None),
    };
    Ok(Some(plan))
}
