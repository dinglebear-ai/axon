use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_done, log_info};
use crate::crates::jobs::common::make_pool;
use crate::crates::services::export::{ExportOptions, export_manifest};
use std::error::Error;

pub async fn run_export(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    let options = ExportOptions {
        include_urls: !cfg.export_no_urls,
        url_limit: cfg.export_url_limit,
        statuses: vec![],
    };

    log_info("Collecting export data from Postgres and Qdrant");
    let manifest = export_manifest(cfg, &pool, &options).await?;
    let json = serde_json::to_string_pretty(&manifest)?;

    if cfg.json_output {
        println!("{json}");
        return Ok(());
    }

    let output_path = cfg.output_path.as_ref().cloned().unwrap_or_else(|| {
        let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        std::path::PathBuf::from(format!("axon-export-{ts}.json"))
    });

    tokio::fs::write(&output_path, json).await?;

    let ingest_total = manifest.ingests.github.len()
        + manifest.ingests.reddit.len()
        + manifest.ingests.youtube.len()
        + manifest.ingests.sessions.len();

    log_done(&format!(
        "export written path={} crawls={} extractions={} embeds={} ingests={} qdrant_points={}",
        output_path.display(),
        manifest.crawls.len(),
        manifest.extractions.len(),
        manifest.embeds.len(),
        ingest_total,
        manifest.qdrant_summary.total_points,
    ));

    Ok(())
}
