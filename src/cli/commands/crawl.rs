mod audit;
mod runtime;
pub(crate) mod subcommands;
mod sync_crawl;

#[cfg(test)]
mod runtime_migration_tests;
#[cfg(test)]
mod sync_backfill_migration_tests;
#[cfg(test)]
mod sync_crawl_migration_tests;

use super::common::parse_urls;
use crate::cli::commands::CommandFuture;
use crate::core::config::{Config, ScrapeFormat};
use crate::core::http::validate_url;
use crate::core::logging::{log_info, log_warn};
use crate::core::ui::{accent, muted, primary, success, warning};
use crate::services::context::ServiceContext;
use crate::services::crawl as crawl_service;
use crate::services::types::CrawlStartJob;
use crate::services::types::StartDisposition;
use spider::url::Url;
use std::error::Error;
use std::path::Path;

// Fire-and-forget jobs (without --wait) require `axon mcp` to be running as a daemon.
// `axon mcp` uses ServiceContext::new_with_workers() which spawns in-process workers.
// CLI commands use ServiceContext::new() (enqueue-only).
pub fn run_crawl<'a>(cfg: &'a Config, service_context: &'a ServiceContext) -> CommandFuture<'a> {
    Box::pin(async move {
        if subcommands::maybe_handle_subcommand(cfg, service_context).await? {
            return Ok(());
        }
        if cfg.format == ScrapeFormat::Llm && !cfg.wait {
            return Err(
                "--format llm requires --wait true for crawl: the LLM-transformed output is streamed to stdout after the crawl completes.\n  Example: axon crawl --format llm --wait true <url> > out.txt".into(),
            );
        }
        let urls = parse_urls(cfg);
        if urls.is_empty() {
            return Err(
                anyhow::anyhow!("crawl requires at least one URL (positional or --urls)").into(),
            );
        }
        for url in &urls {
            validate_url(url)?;
            warn_if_url_looks_like_local_file(url).await;
        }
        let start_url = urls.first().map(String::as_str).unwrap_or("");
        log_info(&format!(
            "command=crawl url={start_url} wait={} render_mode={:?} max_pages={} depth={}",
            cfg.wait, cfg.render_mode, cfg.max_pages, cfg.max_depth
        ));
        if cfg.wait {
            for url in &urls {
                sync_crawl::run_sync_crawl(cfg, url).await?;
            }
            Ok(())
        } else {
            let result = run_async_enqueue_multi(cfg, &urls, service_context).await;
            if result.is_ok() {
                log_info("job_enqueued command=crawl");
            }
            result
        }
    })
}

async fn local_filename_exists_case_insensitive(file_name: &str) -> bool {
    if tokio::fs::try_exists(file_name).await.unwrap_or(false) || Path::new(file_name).exists() {
        return true;
    }
    let Ok(mut entries) = tokio::fs::read_dir(".").await else {
        return false;
    };
    loop {
        let Ok(next) = entries.next_entry().await else {
            return false;
        };
        let Some(entry) = next else {
            return false;
        };
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(file_name)
        {
            return true;
        }
    }
}

async fn warn_if_url_looks_like_local_file(target: &str) {
    let Ok(parsed) = Url::parse(target) else {
        return;
    };
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return;
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return;
    }
    if parsed.path() != "/" && !parsed.path().is_empty() {
        return;
    }
    let Some(host) = parsed.host_str() else {
        return;
    };
    let lower_host = host.to_ascii_lowercase();
    let looks_like_docish_tld = [
        "md", "txt", "rst", "adoc", "json", "yaml", "yml", "toml", "csv", "log", "ini",
    ]
    .iter()
    .any(|suffix| lower_host.ends_with(&format!(".{suffix}")));
    if !looks_like_docish_tld {
        return;
    }
    if !local_filename_exists_case_insensitive(host).await {
        return;
    }
    log_warn(&format!(
        "crawl target {target} looks like a domain that matches local file '{host}'; continuing as web URL"
    ));
}

fn pages_label(max_pages: u32) -> String {
    if max_pages == 0 {
        "uncapped pages".to_string()
    } else {
        format!("{max_pages} pages")
    }
}

fn scope_label(cfg: &Config) -> String {
    let domain = if cfg.include_subdomains {
        "subdomains allowed"
    } else {
        "same domain"
    };
    let mut parts = vec![
        domain.to_string(),
        format!("depth {}", cfg.max_depth),
        pages_label(cfg.max_pages),
    ];
    if cfg.respect_robots {
        parts.push("robots.txt respected".to_string());
    }
    if !cfg.url_whitelist.is_empty() {
        parts.push("URL whitelist active".to_string());
    }
    parts.join(", ")
}

fn strategy_label(cfg: &Config) -> String {
    match cfg.render_mode.to_string().as_str() {
        "auto-switch" => "HTTP first, Chrome fallback".to_string(),
        "chrome" => "Chrome rendering".to_string(),
        "http" => "HTTP only".to_string(),
        other => other.to_string(),
    }
}

fn pipeline_label(cfg: &Config) -> String {
    let mut stages = vec!["crawl"];
    if cfg.discover_sitemaps {
        stages.push("sitemap");
    }
    if cfg.embed {
        stages.push("embed");
    }
    stages.join(" -> ")
}

fn print_summary_row(label: &str, value: &str) {
    println!("  {} {}", muted(&format!("{label:<9}")), value);
}

fn print_override(label: &str, value: &str) {
    println!("  {} {}", muted(&format!("{label:<19}")), warning(value));
}

fn print_crawl_overrides(cfg: &Config) {
    let mut printed_header = false;
    let mut print = |label: &str, value: String| {
        if !printed_header {
            println!();
            println!("{}", primary("Overrides"));
            printed_header = true;
        }
        print_override(label, &value);
    };

    if cfg.respect_robots {
        print("respect robots.txt", "true".to_string());
    }
    if cfg.include_subdomains {
        print("subdomains", "true".to_string());
    }
    if !cfg.url_whitelist.is_empty() {
        print("url whitelist", cfg.url_whitelist.join(", "));
    }
    if let Some(max_page_bytes) = cfg.max_page_bytes {
        print("max page bytes", max_page_bytes.to_string());
    }
    if cfg.block_assets {
        print("block assets", "true".to_string());
    }
    if cfg.redirect_policy_strict {
        print("strict redirects", "true".to_string());
    }
    if cfg.chrome_screenshot {
        print("screenshots", "true".to_string());
    }
    if cfg.bypass_csp {
        print("bypass CSP", "true".to_string());
    }
    if cfg.accept_invalid_certs {
        print("invalid certs", "accepted".to_string());
    }
    if !cfg.cache {
        print("cache", "false".to_string());
    }
}

pub(crate) fn print_async_crawl_result(
    cfg: &Config,
    display: &str,
    jobs: &[CrawlStartJob],
    disposition: StartDisposition,
    via_server: bool,
) {
    let queued = disposition == StartDisposition::Enqueued;
    let headline = if queued {
        success("● Crawl queued")
    } else {
        success("✓ Crawl completed")
    };

    println!("{headline}");
    println!();
    println!("  {}", accent(display));
    println!();
    print_summary_row("Strategy", &strategy_label(cfg));
    print_summary_row("Scope", &scope_label(cfg));
    print_summary_row("Pipeline", &pipeline_label(cfg));
    print_summary_row(
        "Runtime",
        if queued {
            if via_server {
                "server workers"
            } else {
                "background workers"
            }
        } else if via_server {
            "completed on server"
        } else {
            "completed in process"
        },
    );
    println!();

    if jobs.len() == 1 {
        let job = &jobs[0];
        print_summary_row("Job", &accent(&job.job_id));
    } else {
        println!("{}", primary("Jobs"));
        for job in jobs {
            println!(
                "  {} {} {}",
                accent(&job.job_id),
                muted("->"),
                muted(&job.url)
            );
        }
    }

    print_crawl_overrides(cfg);

    if queued {
        println!();
        println!("{}", primary("Follow progress"));
        if jobs.len() == 1 {
            println!(
                "  {}",
                accent(&format!("axon crawl status {}", jobs[0].job_id))
            );
        }
        println!("  {}", accent("axon status"));
    }
}

async fn run_async_enqueue_multi(
    cfg: &Config,
    urls: &[String],
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    // Chrome bootstrap probe belongs to sync crawl — the worker owns Chrome in async mode.
    // Skipping it here eliminates ~10s of failed probe retries on startup.
    let display = match urls {
        [single] => single.clone(),
        _ => format!("{} (+{} more)", urls[0], urls.len() - 1),
    };

    let outcome = crawl_service::crawl_start_with_context(cfg, urls, service_context, None).await?;
    for job in &outcome.result.jobs {
        let status = if outcome.disposition == StartDisposition::Completed {
            "completed"
        } else {
            "pending"
        };
        if cfg.json_output {
            println!(
                "{}",
                serde_json::json!({
                    "url": job.url,
                    "job_id": job.job_id,
                    "status": status,
                    "output_dir": job.output_dir,
                    "predicted_paths": job.predicted_paths,
                })
            );
        }
    }
    if !cfg.json_output {
        print_async_crawl_result(
            cfg,
            &display,
            &outcome.result.jobs,
            outcome.disposition,
            false,
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "crawl_tests.rs"]
mod tests;
