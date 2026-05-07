//! CLI thin wrapper for synchronous crawl — delegates to the services layer.

use crate::core::config::Config;
use crate::services::crawl_sync;
use std::error::Error;

pub(super) async fn run_sync_crawl(cfg: &Config, start_url: &str) -> Result<(), Box<dyn Error>> {
    let _result = crawl_sync::crawl_sync(cfg, start_url).await?;
    Ok(())
}
