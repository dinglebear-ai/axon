#[cfg(test)]
mod scrape_migration_tests;

use super::common::parse_urls;
use crate::core::config::Config;
use crate::core::http::axon_ua;
use crate::core::http::validate_url;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::core::ui::{muted, primary, print_option, print_phase};
use crate::services::scrape as scrape_service;
use crate::vector::ops::tei::{PreparedDoc, embed_prepared_docs};
use futures::stream::{self, StreamExt};
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
pub(crate) async fn scrape_result_to_prepared_doc(
    cfg: &Config,
    result: &crate::services::types::ScrapeResult,
) -> anyhow::Result<PreparedDoc> {
    scrape_service::scrape_result_to_prepared_doc(cfg, result).await
}

fn extractor_label(extractor: &str) -> &str {
    match extractor {
        "crates_io" => "crates.io",
        "docs_rs" => "docs.rs",
        "npm" => "npm",
        "pypi" => "PyPI",
        "github_repo" => "GitHub",
        "github_issue" => "GitHub Issue",
        "github_pr" => "GitHub PR",
        "github_release" => "GitHub Release",
        "huggingface_model" => "Hugging Face",
        "docker_hub" => "Docker Hub",
        "reddit" => "Reddit",
        "hackernews" => "Hacker News",
        "stackoverflow" => "Stack Overflow",
        "dev_to" => "dev.to",
        other => other,
    }
}

fn print_vertical_extra(ex: &serde_json::Value) {
    if let Some(pkg) = ex["pkg_name"].as_str() {
        let ver = ex["pkg_version"].as_str().unwrap_or("?");
        print_option("package", &format!("{pkg} {ver}"));
        if let Some(lang) = ex["pkg_language"].as_str() {
            print_option("language", lang);
        }
        if let Some(author) = ex["pkg_author"].as_str() {
            print_option("author", author);
        }
        if let Some(license) = ex["pkg_license"].as_str() {
            print_option("license", license);
        }
        if let Some(dl) = ex["pkg_downloads"].as_u64() {
            print_option("downloads", &dl.to_string());
        }
        if let Some(n) = ex["docrs_item_count"].as_u64() {
            print_option("items", &format!("{n} public items with documentation"));
        }
    } else if let (Some(owner), Some(repo)) = (ex["owner"].as_str(), ex["repo"].as_str()) {
        print_option("repo", &format!("{owner}/{repo}"));
        if let Some(number) = ex["number"].as_u64() {
            print_option(
                "ref",
                &format!("#{number} ({})", ex["state"].as_str().unwrap_or("?")),
            );
        }
        if let Some(meta) = ex["git_meta"].as_object() {
            if let Some(stars) = meta["stars"].as_u64() {
                print_option("stars", &stars.to_string());
            }
            if let Some(lang) = meta["language"].as_str() {
                print_option("language", lang);
            }
        }
    } else if let Some(model_id) = ex["hf_model_id"].as_str() {
        print_option("model", model_id);
        if let Some(task) = ex["hf_task"].as_str() {
            print_option("task", task);
        }
        if let Some(dl) = ex["hf_downloads"].as_u64() {
            print_option("downloads", &dl.to_string());
        }
        if let Some(likes) = ex["hf_likes"].as_u64() {
            print_option("likes", &likes.to_string());
        }
    } else if let Some(image) = ex["docker_full_name"].as_str() {
        print_option("image", image);
        if let Some(pulls) = ex["docker_pulls"].as_u64() {
            print_option("pulls", &pulls.to_string());
        }
        if let Some(stars) = ex["docker_stars"].as_u64() {
            print_option("stars", &stars.to_string());
        }
    } else if let Some(sub) = ex["reddit_subreddit"].as_str() {
        print_option("subreddit", &format!("r/{sub}"));
        if let Some(author) = ex["reddit_author"].as_str() {
            print_option("author", &format!("u/{author}"));
        }
        if let Some(score) = ex["reddit_score"].as_i64() {
            print_option("score", &score.to_string());
        }
        if let Some(n) = ex["reddit_num_comments"].as_u64() {
            print_option("comments", &n.to_string());
        }
    } else if let Some(hn_type) = ex["hn_type"].as_str() {
        print_option("type", hn_type);
        if let Some(author) = ex["hn_author"].as_str() {
            print_option("author", author);
        }
        if let Some(pts) = ex["hn_points"].as_u64() {
            print_option("points", &pts.to_string());
        }
        if let Some(n) = ex["hn_comment_count"].as_u64() {
            print_option("comments", &n.to_string());
        }
    } else if ex["so_question_id"].is_number() {
        if let Some(score) = ex["so_score"].as_i64() {
            print_option("score", &score.to_string());
        }
        if let Some(ans) = ex["so_answer_count"].as_u64() {
            let suffix = if ex["so_is_answered"].as_bool().unwrap_or(false) {
                " (answered)"
            } else {
                ""
            };
            print_option("answers", &format!("{ans}{suffix}"));
        }
        if let Some(views) = ex["so_view_count"].as_u64() {
            print_option("views", &views.to_string());
        }
    } else if let Some(author) = ex["devto_author"].as_str() {
        print_option("author", author);
        if let Some(mins) = ex["devto_reading_time_mins"].as_u64() {
            print_option("reading time", &format!("{mins} min"));
        }
        if let Some(r) = ex["devto_reactions"].as_u64() {
            print_option("reactions", &r.to_string());
        }
    }
}

fn print_vertical_summary(extractor: &str, extra: Option<&serde_json::Value>) {
    print_option("extractor", extractor_label(extractor));
    if let Some(ex) = extra {
        print_vertical_extra(ex);
    }
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
        if let Some(name) = &result.extractor_name {
            print_vertical_summary(name, result.extra.as_ref());
        }
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
        Ok(Some(scrape_result_to_prepared_doc(cfg, &result).await?))
    } else {
        Ok(None)
    }
}
