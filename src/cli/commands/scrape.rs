#[cfg(test)]
mod scrape_migration_tests;

use super::common::parse_urls;
use crate::core::config::Config;
use crate::core::http::axon_ua;
use crate::core::http::validate_url;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::core::ui::{muted, primary, print_option, print_phase};
use crate::services::scrape as scrape_service;
use crate::vector::ops::input::chunk_markdown;
use crate::vector::ops::tei::{PreparedDoc, embed_prepared_docs};
use futures::stream::{self, StreamExt};
use spider::url::Url as SpiderUrl;
use std::error::Error;

pub(crate) fn print_scrape_preamble(cfg: &Config, url: &str) {
    print_phase("◐", "Scraping", url);
    println!("  {}", primary("Options:"));
    print_option("format", &format!("{:?}", cfg.format));
    print_option("renderMode", &cfg.render_mode.to_string());
    print_option("proxy", cfg.chrome_proxy.as_deref().unwrap_or("none"));
    print_option(
        "userAgent",
        cfg.chrome_user_agent
            .as_deref()
            .unwrap_or_else(|| axon_ua()),
    );
    print_option(
        "timeoutMs",
        &cfg.request_timeout_ms.unwrap_or(20_000).to_string(),
    );
    print_option("fetchRetries", &cfg.fetch_retries.to_string());
    print_option("retryBackoffMs", &cfg.retry_backoff_ms.to_string());
    print_option("indexing", if cfg.embed { "enabled" } else { "skipped" });
    println!();
}

/// Convert a `ScrapeResult` into a `PreparedDoc` for direct embedding.
/// Preserves `extra`, `extractor_name`, and `title` from vertical extractors —
/// these are discarded if we go through the disk-write path instead.
pub(crate) fn scrape_result_to_prepared_doc(
    result: &crate::services::types::ScrapeResult,
) -> PreparedDoc {
    let domain = SpiderUrl::parse(&result.url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    PreparedDoc {
        url: result.url.clone(),
        domain,
        chunks: chunk_markdown(&result.markdown),
        source_type: "scrape".to_string(),
        content_type: "markdown",
        title: result.title.clone(),
        extra: result.extra.clone(),
        extractor_name: result.extractor_name.clone(),
        structured: None,
    }
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
        let doc = dispatch_by_name(name, url, &ctx)
            .await
            .map_err(|e| anyhow::anyhow!("vertical scrape failed: {e}"))?;
        if cfg.json_output {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "url": doc.url, "extractor": doc.extractor_name,
                    "title": doc.title, "markdown": doc.markdown,
                }))?
            );
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
    if let Ok(name) = std::env::var("AXON_VERTICAL")
        && !name.is_empty()
    {
        return run_explicit_vertical(cfg, &name).await;
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
    let mut to_embed: Vec<PreparedDoc> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let results: Vec<_> = stream::iter(&urls)
        .map(|url| scrape_one(cfg, url))
        .buffer_unordered(concurrency)
        .collect()
        .await;
    for result in results {
        match result {
            Ok(Some(doc)) => to_embed.push(doc),
            Ok(None) => {}
            Err(e) => {
                log_warn(&format!("scrape error={e}"));
                errors.push(e.to_string());
            }
        }
    }

    // Phase 2: embed PreparedDocs directly — no disk write, no metadata loss.
    // Vertical extractor fields (extra, extractor_name, title) flow through
    // to Qdrant without being discarded.
    if cfg.embed && !to_embed.is_empty() {
        embed_prepared_docs(cfg, to_embed, None)
            .await
            .map_err(|e| -> Box<dyn Error> { format!("embed failed: {e}").into() })?;
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

/// Scrape one URL, returning `Some(PreparedDoc)` when `cfg.embed` is true.
/// Preserves vertical extractor metadata (extra, extractor_name, title) in the doc.
async fn scrape_one(cfg: &Config, url: &str) -> Result<Option<PreparedDoc>, Box<dyn Error>> {
    print_scrape_preamble(cfg, url);
    validate_url(url)?;
    let result = scrape_service::scrape(cfg, url, None).await?;
    let normalized = result.url.clone();
    let follow_crawl_urls = result.follow_crawl_urls.clone();

    emit_scrape_result(cfg, &result)?;

    // Enqueue follow-up crawl jobs (e.g. docs.rs crawl after crates.io scrape).
    if cfg.embed && !follow_crawl_urls.is_empty() {
        let unique: Vec<&String> = follow_crawl_urls
            .iter()
            .filter(|u| u.as_str() != normalized.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .take(5)
            .collect();
        for follow_url in unique {
            match crate::jobs::crawl::start_crawl_job(cfg, follow_url).await {
                Ok(job_id) => log_info(&format!(
                    "queued follow-up crawl: url={follow_url} job={job_id}"
                )),
                Err(e) => log_warn(&format!(
                    "could not queue follow-up crawl: url={follow_url} err={e}"
                )),
            }
        }
    }

    if cfg.embed {
        Ok(Some(scrape_result_to_prepared_doc(&result)))
    } else {
        Ok(None)
    }
}
