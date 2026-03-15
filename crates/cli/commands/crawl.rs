mod audit;
mod runtime;
mod subcommands;
mod sync_crawl;

#[cfg(test)]
mod runtime_migration_tests;
#[cfg(test)]
mod sync_backfill_migration_tests;
#[cfg(test)]
mod sync_crawl_migration_tests;

use super::common::parse_urls;
use crate::crates::core::config::Config;
use crate::crates::core::http::validate_url;
use crate::crates::core::logging::{log_info, log_warn};
use crate::crates::core::ui::{muted, primary, print_option, print_phase};
use crate::crates::services::crawl as crawl_service;
use spider::url::Url;
use std::error::Error;
use std::path::Path;

pub async fn run_crawl(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if subcommands::maybe_handle_subcommand(cfg).await? {
        return Ok(());
    }
    let urls = parse_urls(cfg);
    if urls.is_empty() {
        return Err("crawl requires at least one URL (positional or --urls)".into());
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
        let result = run_async_enqueue_multi(cfg, &urls).await;
        if result.is_ok() {
            log_info(&format!(
                "job_enqueued command=crawl queue={}",
                cfg.crawl_queue
            ));
        }
        result
    }
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

fn print_async_options(cfg: &Config, start_url: &str) {
    print_phase("◐", "Crawling", start_url);
    println!("  {}", primary("Options:"));
    // Crawl scope
    print_option(
        "maxPages",
        &if cfg.max_pages == 0 {
            "uncapped".to_string()
        } else {
            cfg.max_pages.to_string()
        },
    );
    print_option("maxDepth", &cfg.max_depth.to_string());
    print_option("allowSubdomains", &cfg.include_subdomains.to_string());
    print_option("respectRobotsTxt", &cfg.respect_robots.to_string());
    print_option("discoverSitemaps", &cfg.discover_sitemaps.to_string());
    // Content filtering
    print_option("blockAssets", &cfg.block_assets.to_string());
    print_option(
        "redirectPolicyStrict",
        &cfg.redirect_policy_strict.to_string(),
    );
    print_option(
        "maxPageBytes",
        &cfg.max_page_bytes
            .map(|n| n.to_string())
            .unwrap_or_else(|| "none".to_string()),
    );
    print_option("minMarkdownChars", &cfg.min_markdown_chars.to_string());
    print_option("dropThinMarkdown", &cfg.drop_thin_markdown.to_string());
    if !cfg.url_whitelist.is_empty() {
        print_option("urlWhitelist", &cfg.url_whitelist.join(", "));
    }
    // Render / Chrome
    print_option("renderMode", &cfg.render_mode.to_string());
    print_option("cache", &cfg.cache.to_string());
    print_option("cacheSkipBrowser", &cfg.cache_skip_browser.to_string());
    print_option(
        "chromeRemote",
        cfg.chrome_remote_url.as_deref().unwrap_or("auto/local"),
    );
    print_option("chromeProxy", cfg.chrome_proxy.as_deref().unwrap_or("none"));
    print_option(
        "chromeUserAgent",
        cfg.chrome_user_agent.as_deref().unwrap_or("spider-default"),
    );
    print_option("chromeHeadless", &cfg.chrome_headless.to_string());
    print_option("chromeAntiBot", &cfg.chrome_anti_bot.to_string());
    print_option("chromeStealth", &cfg.chrome_stealth.to_string());
    print_option("chromeIntercept", &cfg.chrome_intercept.to_string());
    print_option("chromeBootstrap", &cfg.chrome_bootstrap.to_string());
    print_option(
        "chromeNetworkIdleTimeoutSecs",
        &cfg.chrome_network_idle_timeout_secs.to_string(),
    );
    print_option(
        "chromeWaitForSelector",
        cfg.chrome_wait_for_selector.as_deref().unwrap_or("none"),
    );
    print_option("chromeScreenshot", &cfg.chrome_screenshot.to_string());
    print_option("bypassCsp", &cfg.bypass_csp.to_string());
    print_option("acceptInvalidCerts", &cfg.accept_invalid_certs.to_string());
    // Output
    print_option("embed", &cfg.embed.to_string());
    print_option("wait", &cfg.wait.to_string());
}

async fn run_async_enqueue_multi(cfg: &Config, urls: &[String]) -> Result<(), Box<dyn Error>> {
    // Chrome bootstrap probe belongs to sync crawl — the worker owns Chrome in async mode.
    // Skipping it here eliminates ~10s of failed probe retries on startup.
    let display = match urls {
        [single] => single.clone(),
        _ => format!("{} (+{} more)", urls[0], urls.len() - 1),
    };
    print_async_options(cfg, &display);
    println!();

    let result = crawl_service::crawl_start(cfg, urls, None).await?;
    println!(
        "  {}",
        muted(
            "Async enqueue mode skips sitemap preflight; worker performs discovery during crawl."
        )
    );
    if cfg.embed {
        println!(
            "  {}",
            muted("Embedding job will be queued automatically after crawl completion.")
        );
    }
    for job in &result.jobs {
        if cfg.json_output {
            println!(
                "{}",
                serde_json::json!({
                    "url": job.url,
                    "job_id": job.job_id,
                    "status": "pending",
                    "output_dir": job.output_dir,
                    "predicted_paths": job.predicted_paths,
                })
            );
        } else {
            println!(
                "  {} {} → {}",
                primary("Crawl Job"),
                crate::crates::core::ui::accent(&job.job_id),
                muted(&job.url)
            );
            println!(
                "  {}",
                muted(&format!("Check status: axon crawl status {}", job.job_id))
            );
        }
    }
    println!();
    if !cfg.json_output {
        for job in &result.jobs {
            println!("Job ID: {}", job.job_id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::local_filename_exists_case_insensitive;
    use serial_test::serial;
    use std::env;
    use std::path::{Path, PathBuf};

    struct CurrentDirGuard {
        original: PathBuf,
    }

    impl CurrentDirGuard {
        fn change_to(path: &Path) -> Self {
            let original = env::current_dir().expect("current dir");
            env::set_current_dir(path).expect("set current dir");
            Self { original }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.original);
        }
    }

    #[tokio::test]
    #[serial]
    async fn local_filename_exists_matches_case_insensitively() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let _guard = CurrentDirGuard::change_to(temp.path());
        tokio::fs::write(temp.path().join("README.MD"), "test")
            .await
            .expect("write file");

        assert!(local_filename_exists_case_insensitive("readme.md").await);
    }
}
