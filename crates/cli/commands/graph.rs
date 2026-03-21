use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::core::ui::{accent, muted, primary};
use crate::crates::services::graph as graph_svc;
use clap::{Parser, Subcommand};
use std::error::Error;

#[derive(Debug, Parser)]
struct GraphRuntimeArgs {
    #[command(subcommand)]
    action: Option<GraphRuntimeSubcommand>,
}

#[derive(Debug, Subcommand)]
enum GraphRuntimeSubcommand {
    Build {
        /// Target URL (positional or --url flag)
        #[arg(value_name = "URL")]
        positional_url: Option<String>,
        #[arg(long = "url", conflicts_with = "positional_url")]
        url: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        all: bool,
    },
    Status,
    Explore {
        entity: String,
    },
    Stats,
    Worker,
}

fn parse_graph_runtime_args(args: &[String]) -> Result<GraphRuntimeArgs, Box<dyn Error>> {
    GraphRuntimeArgs::try_parse_from(
        std::iter::once("graph").chain(args.iter().map(String::as_str)),
    )
    .map_err(|err| err.to_string().into())
}

pub async fn run_graph(cfg: &Config) -> Result<(), Box<dyn Error>> {
    match parse_graph_runtime_args(&cfg.positional)?.action {
        Some(GraphRuntimeSubcommand::Build {
            positional_url,
            url,
            domain,
            all,
        }) => {
            let resolved_url = positional_url.or(url);
            if resolved_url.is_none() && domain.is_none() && !all {
                return Err(anyhow::anyhow!(
                    "graph build requires at least one of: <url>, --url, --domain, or --all"
                )
                .into());
            }
            handle_build(cfg, resolved_url, domain, all).await
        }
        Some(GraphRuntimeSubcommand::Status) => handle_status(cfg).await,
        Some(GraphRuntimeSubcommand::Explore { entity }) => handle_explore(cfg, &entity).await,
        Some(GraphRuntimeSubcommand::Stats) => handle_stats(cfg).await,
        Some(GraphRuntimeSubcommand::Worker) => handle_worker(cfg).await,
        None => {
            Err(anyhow::anyhow!("Usage: axon graph <build|status|explore|stats|worker>").into())
        }
    }
}

async fn handle_build(
    cfg: &Config,
    url: Option<String>,
    domain: Option<String>,
    all: bool,
) -> Result<(), Box<dyn Error>> {
    log_info(&format!(
        "command=graph.build url={} domain={} all={all}",
        url.as_deref().unwrap_or(""),
        domain.as_deref().unwrap_or("")
    ));
    let result = graph_svc::graph_build(cfg, url.as_deref(), domain.as_deref(), all).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
        return Ok(());
    }

    let queued = result.payload["queued"].as_u64().unwrap_or(0);
    println!(
        "{} {}",
        primary("Graph build queued:"),
        accent(&queued.to_string())
    );
    if let Some(urls) = result.payload["urls"].as_array()
        && !urls.is_empty()
    {
        for url in urls.iter().take(10) {
            if let Some(url) = url.as_str() {
                println!("  {}", muted(url));
            }
        }
        if urls.len() > 10 {
            println!("  {}", muted("..."));
        }
    }
    Ok(())
}

async fn handle_status(cfg: &Config) -> Result<(), Box<dyn Error>> {
    log_info("command=graph.status");
    let result = graph_svc::graph_status(cfg).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
        return Ok(());
    }

    println!("{}", primary("Graph Extraction Status:"));
    if let Some(counts) = result.payload["counts"].as_object() {
        for key in ["completed", "running", "pending", "failed", "canceled"] {
            let count = counts.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
            println!("  {:<10} {}", format!("{key}:"), accent(&count.to_string()));
        }
    }
    if let Some(recent) = result.payload["recent"].as_array()
        && !recent.is_empty()
    {
        println!();
        println!("{}", primary("Recent jobs:"));
        for item in recent.iter().take(10) {
            let url = item["url"].as_str().unwrap_or("?");
            let status = item["status"].as_str().unwrap_or("unknown");
            println!("  {} {}", accent(status), muted(url));
        }
    }
    Ok(())
}

async fn handle_explore(cfg: &Config, entity: &str) -> Result<(), Box<dyn Error>> {
    log_info(&format!("command=graph.explore entity={entity}"));
    let result = graph_svc::graph_explore(cfg, entity).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
        return Ok(());
    }

    println!("{} {}", primary("Graph explore:"), accent(entity));
    if let Some(rows) = result.payload["rows"].as_array() {
        if rows.is_empty() {
            println!("  {}", muted("No graph entity found."));
            return Ok(());
        }
        for row in rows.iter().take(5) {
            println!("  {}", serde_json::to_string_pretty(row)?);
        }
    }
    Ok(())
}

async fn handle_stats(cfg: &Config) -> Result<(), Box<dyn Error>> {
    log_info("command=graph.stats");
    let result = graph_svc::graph_stats(cfg).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
        return Ok(());
    }

    println!("{}", primary("Graph stats:"));
    if let Some(rows) = result.payload["rows"].as_array() {
        if rows.is_empty() {
            println!("  {}", muted("No graph stats available."));
        } else {
            for row in rows.iter().take(5) {
                println!("  {}", serde_json::to_string_pretty(row)?);
            }
        }
    }
    Ok(())
}

async fn handle_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    log_info("command=graph.worker");
    graph_svc::graph_worker(cfg).await
}

#[cfg(test)]
mod tests {
    use super::{GraphRuntimeSubcommand, parse_graph_runtime_args};

    #[test]
    fn parse_graph_runtime_args_accepts_build_flags() {
        let args = vec![
            "build".to_string(),
            "--url".to_string(),
            "https://example.com".to_string(),
            "--domain".to_string(),
            "example.com".to_string(),
            "--all".to_string(),
        ];
        let parsed = parse_graph_runtime_args(&args).expect("valid args");
        match parsed.action {
            Some(GraphRuntimeSubcommand::Build {
                url, domain, all, ..
            }) => {
                assert_eq!(url.as_deref(), Some("https://example.com"));
                assert_eq!(domain.as_deref(), Some("example.com"));
                assert!(all);
            }
            other => panic!("unexpected action: {other:?}"),
        }
    }

    #[test]
    fn parse_graph_runtime_args_accepts_positional_url() {
        let args = vec![
            "build".to_string(),
            "https://positional.example.com".to_string(),
        ];
        let parsed = parse_graph_runtime_args(&args).expect("valid args");
        match parsed.action {
            Some(GraphRuntimeSubcommand::Build {
                positional_url,
                url,
                ..
            }) => {
                assert_eq!(
                    positional_url.as_deref(),
                    Some("https://positional.example.com")
                );
                assert!(url.is_none());
            }
            other => panic!("unexpected action: {other:?}"),
        }
    }

    #[test]
    fn parse_graph_runtime_args_rejects_missing_flag_value() {
        let args = vec!["build".to_string(), "--domain".to_string()];
        let err = parse_graph_runtime_args(&args).expect_err("missing domain should error");
        assert!(err.to_string().contains("--domain"));
    }

    #[tokio::test]
    async fn run_graph_build_requires_at_least_one_target() {
        use crate::crates::core::config::Config;
        let mut cfg = Config::test_default();
        cfg.positional = vec!["build".to_string()];
        let err = super::run_graph(&cfg)
            .await
            .expect_err("no target should error");
        assert!(
            err.to_string()
                .contains("requires at least one of: <url>, --url, --domain, or --all")
        );
    }

    #[tokio::test]
    async fn run_graph_without_subcommand_errors() {
        use crate::crates::core::config::Config;
        let mut cfg = Config::test_default();
        cfg.positional = vec![];
        let err = super::run_graph(&cfg)
            .await
            .expect_err("missing subcommand should error");
        assert!(
            err.to_string()
                .contains("Usage: axon graph <build|status|explore|stats|worker>")
        );
    }
}
