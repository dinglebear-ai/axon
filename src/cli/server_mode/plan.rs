use crate::cli;
use crate::core::config::{CommandKind, Config};
use crate::services::client_contract::{
    RestAskRequest, RestCrawlRequest, RestEmbedRequest, RestEvaluateRequest, RestExtractRequest,
    RestMapRequest, RestQueryRequest, RestRetrieveRequest, RestScrapeRequest, RestSearchRequest,
    RestSuggestRequest, RestSummarizeRequest,
};
use std::error::Error;
use std::fmt;

use super::{ServerJobFamily, server_mode_rejects_host_local_embed_input};

#[path = "plan_ingest.rs"]
mod plan_ingest;
use plan_ingest::ingest_server_rest_plan;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ServerRestPlan {
    pub method: &'static str,
    pub path: String,
    pub body: serde_json::Value,
    pub label: &'static str,
    pub poll_family: Option<ServerJobFamily>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServerPlanError(String);

impl ServerPlanError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for ServerPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for ServerPlanError {}

pub(crate) fn status_path_for_family(family: ServerJobFamily, job_id: &str) -> String {
    match family {
        ServerJobFamily::Crawl => format!("/v1/crawl/{job_id}"),
        ServerJobFamily::Extract => format!("/v1/extract/{job_id}"),
        ServerJobFamily::Embed => format!("/v1/embed/{job_id}"),
        ServerJobFamily::Ingest => format!("/v1/ingest/{job_id}"),
    }
}

pub(crate) fn server_rest_plan(cfg: &Config) -> Result<ServerRestPlan, ServerPlanError> {
    if let Some(plan) = discovery_rest_plan(cfg) {
        return Ok(plan);
    }
    if let Some(plan) = query_rest_plan(cfg)? {
        return Ok(plan);
    }

    match cfg.command {
        CommandKind::Scrape => {
            let url = single_url(cfg, "scrape")?;
            Ok(ServerRestPlan {
                method: "POST",
                path: "/v1/scrape".to_string(),
                body: json_body(RestScrapeRequest {
                    url: Some(url),
                    urls: None,
                    render_mode: Some(cfg.render_mode),
                    format: Some(cfg.format),
                    embed: Some(cfg.embed),
                    collection: Some(cfg.collection.clone()),
                    root_selector: cfg.root_selector.clone(),
                    exclude_selector: cfg.exclude_selector.clone(),
                    headers: cfg.custom_headers.clone(),
                })?,
                label: "scrape",
                poll_family: None,
            })
        }
        CommandKind::Summarize => {
            let urls = cli::commands::common::parse_urls(cfg);
            if urls.is_empty() {
                return Err(ServerPlanError::new(
                    "summarize requires at least one URL (positional or --urls)",
                ));
            }
            Ok(ServerRestPlan {
                method: "POST",
                path: "/v1/summarize".to_string(),
                body: json_body(RestSummarizeRequest {
                    url: None,
                    urls: Some(urls),
                    render_mode: Some(cfg.render_mode),
                    root_selector: cfg.root_selector.clone(),
                    exclude_selector: cfg.exclude_selector.clone(),
                    headers: cfg.custom_headers.clone(),
                })?,
                label: "summarize",
                poll_family: None,
            })
        }
        CommandKind::Crawl => crawl_server_rest_plan(cfg),
        CommandKind::Extract => extract_server_rest_plan(cfg),
        CommandKind::Embed => embed_server_rest_plan(cfg),
        CommandKind::Ingest => ingest_server_rest_plan(cfg, false),
        CommandKind::Sessions => ingest_server_rest_plan(cfg, true),
        _ => Err(ServerPlanError::new(format!(
            "{} is not routed through server mode",
            cfg.command
        ))),
    }
}

fn discovery_rest_plan(cfg: &Config) -> Option<ServerRestPlan> {
    match cfg.command {
        CommandKind::Status => Some(ServerRestPlan {
            method: "GET",
            path: "/v1/status".to_string(),
            body: serde_json::Value::Null,
            label: "status",
            poll_family: None,
        }),
        CommandKind::Doctor => Some(ServerRestPlan {
            method: "GET",
            path: "/v1/doctor".to_string(),
            body: serde_json::Value::Null,
            label: "doctor",
            poll_family: None,
        }),
        CommandKind::Sources => Some(ServerRestPlan {
            method: "GET",
            path: page_path(
                "/v1/sources",
                Some(if cfg.sources_domain_all {
                    10_000
                } else {
                    cfg.search_limit
                }),
                None,
                cfg.sources_domain.as_deref(),
                None,
            ),
            body: serde_json::Value::Null,
            label: "sources",
            poll_family: None,
        }),
        CommandKind::Domains => Some(ServerRestPlan {
            method: "GET",
            path: page_path(
                "/v1/domains",
                Some(cfg.search_limit),
                None,
                cfg.domains_domain.as_deref(),
                None,
            ),
            body: serde_json::Value::Null,
            label: "domains",
            poll_family: None,
        }),
        CommandKind::Stats => Some(ServerRestPlan {
            method: "GET",
            path: "/v1/stats".to_string(),
            body: serde_json::Value::Null,
            label: "stats",
            poll_family: None,
        }),
        _ => None,
    }
}

fn query_rest_plan(cfg: &Config) -> Result<Option<ServerRestPlan>, ServerPlanError> {
    let plan = match cfg.command {
        CommandKind::Map => {
            let url = single_url(cfg, "map")?;
            ServerRestPlan {
                method: "POST",
                path: "/v1/map".to_string(),
                body: json_body(RestMapRequest {
                    url,
                    limit: Some(cfg.search_limit),
                    offset: Some(0),
                })?,
                label: "map",
                poll_family: None,
            }
        }
        CommandKind::Query => {
            let query = query_text(cfg, "query")?;
            ServerRestPlan {
                method: "POST",
                path: "/v1/query".to_string(),
                body: json_body(RestQueryRequest {
                    query,
                    collection: Some(cfg.collection.clone()),
                    limit: Some(cfg.search_limit),
                    offset: Some(0),
                })?,
                label: "query",
                poll_family: None,
            }
        }
        CommandKind::Retrieve => {
            let url = single_url(cfg, "retrieve")?;
            ServerRestPlan {
                method: "POST",
                path: "/v1/retrieve".to_string(),
                body: json_body(RestRetrieveRequest {
                    url,
                    collection: Some(cfg.collection.clone()),
                    max_points: cfg.retrieve_max_points,
                    cursor: None,
                    token_budget: None,
                })?,
                label: "retrieve",
                poll_family: None,
            }
        }
        CommandKind::Ask => {
            let query = query_text(cfg, "ask")?;
            ServerRestPlan {
                method: "POST",
                path: "/v1/ask".to_string(),
                body: json_body(RestAskRequest {
                    query,
                    collection: Some(cfg.collection.clone()),
                    since: cfg.since.clone(),
                    before: cfg.before.clone(),
                    diagnostics: Some(cfg.ask_diagnostics),
                    explain: Some(cfg.ask_explain),
                    hybrid_search: Some(cfg.hybrid_search_enabled),
                    ask_chunk_limit: Some(cfg.ask_chunk_limit),
                    ask_full_docs: Some(cfg.ask_full_docs),
                    ask_max_context_chars: Some(cfg.ask_max_context_chars),
                    ask_hybrid_candidates: Some(cfg.ask_hybrid_candidates),
                    ask_min_relevance_score: Some(cfg.ask_min_relevance_score),
                    ask_doc_chunk_limit: Some(cfg.ask_doc_chunk_limit),
                    ask_doc_fetch_concurrency: Some(cfg.ask_doc_fetch_concurrency),
                    ask_backfill_chunks: Some(cfg.ask_backfill_chunks),
                    ask_candidate_limit: Some(cfg.ask_candidate_limit),
                    ask_min_citations_nontrivial: Some(cfg.ask_min_citations_nontrivial),
                    ask_authoritative_domains: Some(cfg.ask_authoritative_domains.clone()),
                    ask_authoritative_boost: Some(cfg.ask_authoritative_boost),
                })?,
                label: "ask",
                poll_family: None,
            }
        }
        CommandKind::Evaluate => {
            let question = query_text(cfg, "evaluate")?;
            ServerRestPlan {
                method: "POST",
                path: "/v1/evaluate".to_string(),
                body: json_body(RestEvaluateRequest {
                    question,
                    collection: Some(cfg.collection.clone()),
                })?,
                label: "evaluate",
                poll_family: None,
            }
        }
        CommandKind::Suggest => ServerRestPlan {
            method: "POST",
            path: "/v1/suggest".to_string(),
            body: json_body(RestSuggestRequest {
                focus: cfg.query.clone(),
                collection: Some(cfg.collection.clone()),
            })?,
            label: "suggest",
            poll_family: None,
        },
        CommandKind::Search => {
            let query = query_text(cfg, "search")?;
            search_like_plan("search", "/v1/search", query, cfg)
        }
        CommandKind::Research => {
            let query = query_text(cfg, "research")?;
            search_like_plan("research", "/v1/research", query, cfg)
        }
        _ => return Ok(None),
    };
    Ok(Some(plan))
}

fn crawl_server_rest_plan(cfg: &Config) -> Result<ServerRestPlan, ServerPlanError> {
    if let Some(subaction) = cfg.positional.first().map(String::as_str)
        && let Some(plan) =
            async_job_lifecycle_plan("crawl", ServerJobFamily::Crawl, subaction, cfg)?
    {
        return Ok(plan);
    }
    let urls = cli::commands::common::parse_urls(cfg);
    if urls.is_empty() {
        return Err(ServerPlanError::new(
            "crawl requires at least one URL (positional or --urls)",
        ));
    }
    Ok(ServerRestPlan {
        method: "POST",
        path: "/v1/crawl".to_string(),
        body: json_body(RestCrawlRequest {
            urls,
            max_pages: Some(cfg.max_pages),
            max_depth: Some(cfg.max_depth),
            render_mode: Some(cfg.render_mode),
            include_subdomains: Some(cfg.include_subdomains),
            respect_robots: Some(cfg.respect_robots),
            discover_sitemaps: Some(cfg.discover_sitemaps),
            max_sitemaps: Some(cfg.max_sitemaps),
            sitemap_since_days: Some(cfg.sitemap_since_days),
            delay_ms: Some(cfg.delay_ms),
            collection: Some(cfg.collection.clone()),
            headers: cfg.custom_headers.clone(),
        })?,
        label: "crawl",
        poll_family: Some(ServerJobFamily::Crawl),
    })
}

fn extract_server_rest_plan(cfg: &Config) -> Result<ServerRestPlan, ServerPlanError> {
    if let Some(subaction) = cfg.positional.first().map(String::as_str)
        && let Some(plan) =
            async_job_lifecycle_plan("extract", ServerJobFamily::Extract, subaction, cfg)?
    {
        return Ok(plan);
    }
    let urls = cli::commands::common::parse_urls(cfg);
    if urls.is_empty() {
        return Err(ServerPlanError::new(
            "extract requires at least one URL (positional or --urls)",
        ));
    }
    Ok(ServerRestPlan {
        method: "POST",
        path: "/v1/extract".to_string(),
        body: json_body(RestExtractRequest {
            urls,
            prompt: cfg.query.clone(),
            mode: None,
            max_pages: Some(cfg.max_pages),
            render_mode: Some(cfg.render_mode),
            embed: Some(cfg.embed),
            collection: Some(cfg.collection.clone()),
            headers: cfg.custom_headers.clone(),
        })?,
        label: "extract",
        poll_family: Some(ServerJobFamily::Extract),
    })
}

fn embed_server_rest_plan(cfg: &Config) -> Result<ServerRestPlan, ServerPlanError> {
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
        return Err(ServerPlanError::new(
            "server mode does not accept host-local embed paths yet; use a URL/text input or `--local`",
        ));
    }
    Ok(ServerRestPlan {
        method: "POST",
        path: "/v1/embed".to_string(),
        body: json_body(RestEmbedRequest {
            input,
            source_type: None,
            collection: Some(cfg.collection.clone()),
        })?,
        label: "embed",
        poll_family: Some(ServerJobFamily::Embed),
    })
}

fn single_url(cfg: &Config, command: &str) -> Result<String, ServerPlanError> {
    let urls = cli::commands::common::parse_urls(cfg);
    match urls.as_slice() {
        [] => Err(ServerPlanError::new(format!("{command} requires a URL"))),
        [url] => Ok(url.clone()),
        _ => Err(ServerPlanError::new(format!(
            "{command} accepts exactly one URL in server mode"
        ))),
    }
}

fn query_text(cfg: &Config, command: &str) -> Result<String, ServerPlanError> {
    cli::commands::resolve_input_text(cfg)
        .ok_or_else(|| ServerPlanError::new(format!("{command} requires text")))
}

fn search_like_plan(
    label: &'static str,
    path: &'static str,
    query: String,
    cfg: &Config,
) -> ServerRestPlan {
    ServerRestPlan {
        method: "POST",
        path: path.to_string(),
        body: serde_json::to_value(RestSearchRequest {
            query,
            limit: Some(cfg.search_limit),
            offset: Some(0),
            time_range: None,
        })
        .unwrap_or(serde_json::Value::Null),
        label,
        poll_family: None,
    }
}

pub(super) fn json_body<T: serde::Serialize>(
    value: T,
) -> Result<serde_json::Value, ServerPlanError> {
    serde_json::to_value(value)
        .map_err(|err| ServerPlanError::new(format!("failed to serialize REST request: {err}")))
}

fn page_path(
    base: &str,
    limit: Option<usize>,
    offset: Option<usize>,
    domain: Option<&str>,
    cursor: Option<&str>,
) -> String {
    let mut pairs = Vec::new();
    if let Some(limit) = limit {
        pairs.push(("limit".to_string(), limit.to_string()));
    }
    if let Some(offset) = offset {
        pairs.push(("offset".to_string(), offset.to_string()));
    }
    if let Some(domain) = domain {
        pairs.push(("domain".to_string(), domain.to_string()));
    }
    if let Some(cursor) = cursor {
        pairs.push(("cursor".to_string(), cursor.to_string()));
    }
    if pairs.is_empty() {
        base.to_string()
    } else {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in pairs {
            serializer.append_pair(&key, &value);
        }
        format!("{base}?{}", serializer.finish())
    }
}

fn async_job_lifecycle_plan(
    family: &'static str,
    poll_family: ServerJobFamily,
    subaction: &str,
    cfg: &Config,
) -> Result<Option<ServerRestPlan>, ServerPlanError> {
    let plan = match subaction {
        "status" | "errors" => {
            let id = cfg.positional.get(1).ok_or_else(|| {
                ServerPlanError::new(format!("{family} {subaction} requires <job-id>"))
            })?;
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
            let id = cfg.positional.get(1).ok_or_else(|| {
                ServerPlanError::new(format!("{family} cancel requires <job-id>"))
            })?;
            ServerRestPlan {
                method: "POST",
                path: format!("/v1/{family}/{id}/cancel"),
                body: serde_json::json!({}),
                label: family,
                poll_family: None,
            }
        }
        "worker" => {
            return Err(ServerPlanError::new(format!(
                "server mode does not start local {family} workers; use `axon serve`"
            )));
        }
        _ => return Ok(None),
    };
    Ok(Some(plan))
}
