use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_done, log_info};
use crate::crates::jobs::common::make_pool;
use crate::crates::services::export::{ExportOptions, export_manifest, verify_manifest_json};
use std::error::Error;

pub async fn run_export(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if let Some(path) = &cfg.export_verify_input {
        let raw = tokio::fs::read_to_string(path).await?;
        let report = verify_manifest_json(&raw)?;
        if cfg.json_output {
            println!("{}", serde_json::to_string_pretty(&report)?);
            if report.valid {
                return Ok(());
            }
            return Err(format!(
                "export verify failed path={} missing_keys={} parse_error={} hash_mismatches={} count_mismatches={}",
                path.display(),
                report.missing_required_keys.len(),
                report.parse_error.as_deref().unwrap_or("none"),
                report.hash_mismatches.len(),
                report.count_mismatches.len(),
            )
            .into());
        }
        if report.valid {
            log_done(&format!(
                "export verify passed path={} schema_version={}",
                path.display(),
                report.version.unwrap_or_default()
            ));
            return Ok(());
        }
        return Err(format!(
            "export verify failed path={} missing_keys={} parse_error={} hash_mismatches={} count_mismatches={}",
            path.display(),
            report.missing_required_keys.len(),
            report.parse_error.as_deref().unwrap_or("none"),
            report.hash_mismatches.len(),
            report.count_mismatches.len(),
        )
        .into());
    }

    let pool = make_pool(cfg).await?;
    let options = ExportOptions {
        include_history: cfg.export_include_history,
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
