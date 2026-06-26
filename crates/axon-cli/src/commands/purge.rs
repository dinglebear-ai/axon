//! CLI wrapper for deleting indexed Qdrant points by URL.
//!
//! Thin shim over `services::system::purge` — the CLI owns only the dry-run
//! preview prompt and output formatting; all delete logic lives in the service
//! layer (shared with the MCP and REST surfaces).

use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::{accent, confirm_destructive, muted, primary, symbol_for_status};
use axon_services::system::purge;
use axon_services::types::PurgeResult;
use std::error::Error;

pub async fn run_purge(cfg: &Config) -> Result<(), Box<dyn Error>> {
    log_info("command=purge");
    let target = cfg
        .positional
        .first()
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or("purge requires a URL argument")?;

    let preview = purge(cfg, target, cfg.purge_prefix, true).await?;
    if cfg.purge_dry_run {
        return report(cfg, &preview);
    }

    let prompt = format!(
        "Delete {} Qdrant point(s) across {} indexed URL(s) from collection '{}'?",
        preview.matched_points, preview.matched_url_count, cfg.collection
    );
    if !confirm_destructive(cfg, &prompt)? {
        if cfg.json_output {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": false,
                    "aborted": true,
                    "target": preview.target,
                    "prefix": preview.prefix,
                    "matched_points": preview.matched_points,
                    "matched_url_count": preview.matched_url_count,
                    "sample_urls": preview.sample_urls,
                }))?
            );
        } else {
            println!("{} purge aborted", symbol_for_status("canceled"));
        }
        return Ok(());
    }

    let result = purge(cfg, target, cfg.purge_prefix, false).await?;
    report(cfg, &result)
}

fn report(cfg: &Config, result: &PurgeResult) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "target": result.target,
                "prefix": result.prefix,
                "dry_run": result.dry_run,
                "matched_points": result.matched_points,
                "deleted_points": result.deleted_points,
                "matched_url_count": result.matched_url_count,
                "sample_urls": result.sample_urls,
                "collection": cfg.collection,
            }))?
        );
        return Ok(());
    }

    let action = if result.dry_run {
        "would delete"
    } else {
        "deleted"
    };
    let point_count = if result.dry_run {
        result.matched_points
    } else {
        result.deleted_points
    };
    println!(
        "{} {} {} {} Qdrant point(s) across {} indexed URL(s) from {}",
        symbol_for_status("completed"),
        primary("purge"),
        action,
        point_count,
        result.matched_url_count,
        accent(&cfg.collection)
    );
    println!("{} target: {}", muted("·"), result.target);
    if result.prefix {
        println!("{} prefix matching enabled", muted("·"));
    }
    if !result.sample_urls.is_empty() {
        println!("{}", muted("Sample matched URLs:"));
        for url in &result.sample_urls {
            println!("  {url}");
        }
    }
    Ok(())
}
