#[cfg(test)]
mod scrape_migration_tests;

use super::common::parse_urls;
use crate::core::config::Config;
use crate::core::content::url_to_filename;
use crate::core::http::validate_url;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::core::ui::{muted, primary, print_option, print_phase};
use crate::services::embed as embed_service;
use crate::services::scrape as scrape_service;
use futures::stream::{self, StreamExt};
use std::error::Error;
use uuid::Uuid;

pub(crate) fn print_scrape_preamble(cfg: &Config, url: &str) {
    print_phase("◐", "Scraping", url);
    println!("  {}", primary("Options:"));
    print_option("format", &format!("{:?}", cfg.format));
    print_option("renderMode", &cfg.render_mode.to_string());
    print_option("proxy", cfg.chrome_proxy.as_deref().unwrap_or("none"));
    print_option(
        "userAgent",
        cfg.chrome_user_agent.as_deref().unwrap_or("spider-default"),
    );
    print_option(
        "timeoutMs",
        &cfg.request_timeout_ms.unwrap_or(20_000).to_string(),
    );
    print_option("fetchRetries", &cfg.fetch_retries.to_string());
    print_option("retryBackoffMs", &cfg.retry_backoff_ms.to_string());
    print_option("chromeAntiBot", &cfg.chrome_anti_bot.to_string());
    print_option("chromeStealth", &cfg.chrome_stealth.to_string());
    print_option("chromeIntercept", &cfg.chrome_intercept.to_string());
    print_option("embed", &cfg.embed.to_string());
    println!();
}

pub(crate) fn emit_scrape_result(
    cfg: &Config,
    result: &crate::services::types::ScrapeResult,
) -> Result<(), Box<dyn Error>> {
    let normalized = &result.url;
    let bytes = result.output.len();
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
        log_done(&format!(
            "command=scrape url={normalized} bytes={bytes} format={:?}",
            cfg.format
        ));
    } else if let Some(path) = &cfg.output_path {
        std::fs::write(path, &result.output)?;
        log_done(&format!(
            "wrote output: {} url={normalized} bytes={bytes} format={:?}",
            path.to_string_lossy(),
            cfg.format
        ));
    } else {
        println!("{} {}", primary("Scrape Results for"), normalized);
        println!("{}\n", muted("As of: now"));
        println!("{}", result.output);
        log_done(&format!(
            "command=scrape url={normalized} bytes={bytes} format={:?}",
            cfg.format
        ));
    }
    Ok(())
}

async fn run_explicit_vertical(cfg: &Config, name: &str) -> Result<(), Box<dyn Error>> {
    use crate::extract::{VerticalContext, dispatch_by_name};
    let urls = parse_urls(cfg);
    if urls.is_empty() {
        return Err(anyhow::anyhow!("scrape requires at least one URL").into());
    }
    let ctx = VerticalContext::new(std::sync::Arc::new(cfg.clone()));
    for url in &urls {
        let doc = dispatch_by_name(name, url, &ctx).await
            .map_err(|e| anyhow::anyhow!("vertical scrape failed: {e}"))?;
        if cfg.json_output {
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "url": doc.url, "extractor": doc.extractor_name,
                "title": doc.title, "markdown": doc.markdown,
            }))?);
        } else {
            println!("{}", doc.markdown);
        }
    }
    Ok(())
}

pub async fn run_scrape(cfg: &Config) -> Result<(), Box<dyn Error>> {
    // Explicit vertical override for auto_dispatch=false extractors (amazon, ebay, youtube).
    // Transparent auto-dispatch for auto_dispatch=true extractors happens inside
    // services::scrape::scrape() — no env var needed for those.
    // Usage: AXON_VERTICAL=amazon axon scrape https://amazon.com/dp/{asin} --local
    if let Ok(name) = std::env::var("AXON_VERTICAL") {
        if !name.is_empty() {
            return run_explicit_vertical(cfg, &name).await;
        }
    }

    let urls = parse_urls(cfg);
    if urls.is_empty() {
        return Err(
            anyhow::anyhow!("scrape requires at least one URL (positional or --urls)").into(),
        );
    }
    if cfg.output_path.is_some() && urls.len() > 1 {
        return Err(anyhow::anyhow!(
            "--output cannot be used with multiple URLs (each would overwrite the same file)"
        )
        .into());
    }
    log_info(&format!(
        "command=scrape urls={} format={:?} wait={}",
        urls.len(),
        cfg.format,
        cfg.wait
    ));

    // Phase 1: scrape URLs concurrently, bounded by batch_concurrency.
    let concurrency = cfg.batch_concurrency.max(1);
    let mut to_embed: Vec<(String, String)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let results: Vec<_> = stream::iter(&urls)
        .map(|url| scrape_one(cfg, url))
        .buffer_unordered(concurrency)
        .collect()
        .await;
    for result in results {
        match result {
            Ok(Some(pair)) => to_embed.push(pair),
            Ok(None) => {}
            Err(e) => {
                log_warn(&format!("scrape error={e}"));
                errors.push(e.to_string());
            }
        }
    }

    // Phase 2: embed all collected markdowns in one batch.
    // Important: write this run's files into an isolated directory so `scrape --embed`
    // only indexes current outputs, not every historical file in scrape-markdown.
    if cfg.embed && !to_embed.is_empty() {
        let run_id = Uuid::new_v4().to_string();
        let embed_dir = cfg
            .output_dir
            .join("scrape-markdown")
            .join("runs")
            .join(run_id);
        tokio::fs::create_dir_all(&embed_dir).await?;
        for (normalized, markdown) in &to_embed {
            tokio::fs::write(embed_dir.join(url_to_filename(normalized, 1)), markdown).await?;
        }
        embed_service::embed_now_with_source(cfg, &embed_dir.to_string_lossy(), Some("scrape"))
            .await?;
    }

    if !errors.is_empty() {
        return Err(format!(
            "{} scrape(s) failed:\n  {}",
            errors.len(),
            errors.join("\n  ")
        )
        .into());
    }

    Ok(())
}

/// Returns `Some((normalized_url, markdown))` when `cfg.embed` is true so the
/// caller can batch-embed after all scrapes complete. Returns `None` otherwise.
async fn scrape_one(cfg: &Config, url: &str) -> Result<Option<(String, String)>, Box<dyn Error>> {
    print_scrape_preamble(cfg, url);

    // SSRF guard: validate before creating Website — must run before any
    // network activity so private-IP seeds are rejected immediately.
    validate_url(url)?;
    let result = scrape_service::scrape(cfg, url, None).await?;
    let normalized = result.url.clone();
    let markdown = result.markdown.clone();

    emit_scrape_result(cfg, &result)?;

    if cfg.embed {
        Ok(Some((normalized, markdown)))
    } else {
        Ok(None)
    }
}
