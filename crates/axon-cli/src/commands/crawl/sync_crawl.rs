//! CLI thin wrapper for synchronous crawl — delegates to the services layer.

use axon_core::config::Config;
use axon_core::ui::panel;
use axon_services::crawl_sync;
use std::error::Error;

pub(super) async fn run_sync_crawl(cfg: &Config, start_url: &str) -> Result<(), Box<dyn Error>> {
    let result = crawl_sync::crawl_sync(cfg, start_url).await?;
    if !cfg.json_output {
        let pages = result.pages_seen.to_string();
        let markdown = result.markdown_files.to_string();
        let thin = result.thin_pages.to_string();
        let errors = result.error_pages.to_string();
        let elapsed = format!("{:.1}s", result.elapsed_ms as f64 / 1000.0);
        let title = if result.cache_hit {
            "Crawl complete (cache)"
        } else {
            "Crawl complete"
        };
        println!(
            "{}",
            panel(
                title,
                &[
                    ("pages", pages.as_str()),
                    ("markdown", markdown.as_str()),
                    ("thin", thin.as_str()),
                    ("errors", errors.as_str()),
                    ("elapsed", elapsed.as_str()),
                ],
            )
        );
    }
    Ok(())
}
