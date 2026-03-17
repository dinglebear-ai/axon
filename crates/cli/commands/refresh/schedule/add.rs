use super::{REFRESH_TIER_MEDIUM_SECONDS, schedule_name_arg, tier_to_seconds};
use crate::crates::core::config::Config;
use crate::crates::core::http::validate_url;
use crate::crates::core::ui::{accent, muted, symbol_for_status};
use crate::crates::jobs::watch::{WatchDefCreate, create_watch_def};
use crate::crates::services::refresh as refresh_service;
use chrono::{Duration, Utc};
use std::error::Error;

struct RefreshScheduleAddInput {
    name: String,
    seed_url: Option<String>,
    urls: Option<Vec<String>>,
    every_seconds: i64,
}

pub(super) async fn handle_refresh_schedule_add(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let name = schedule_name_arg(cfg, "add", 2)?;
    if let Some(repo) = name.strip_prefix("github:") {
        let parts: Vec<&str> = repo.split('/').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(
                anyhow::anyhow!("Invalid GitHub target. Expected: github:owner/repo").into(),
            );
        }
        return super::super::github::handle_refresh_schedule_add_github(cfg, repo, &name).await;
    }

    let input = parse_refresh_schedule_add_input(cfg, name).await?;
    let schedule = build_refresh_schedule_create(&input);
    let created = refresh_service::refresh_schedule_create(cfg, &schedule).await?;
    create_refresh_watch_def(cfg, &created).await?;
    print_refresh_schedule_add_result(cfg, &created)?;
    Ok(())
}

async fn parse_refresh_schedule_add_input(
    cfg: &Config,
    name: String,
) -> Result<RefreshScheduleAddInput, Box<dyn Error>> {
    let mut seed_url: Option<String> = None;
    let mut every_seconds: Option<i64> = None;
    let mut tier_seconds: Option<i64> = None;
    let mut urls: Option<Vec<String>> = None;

    let mut idx = 3usize;
    while idx < cfg.positional.len() {
        match cfg.positional[idx].as_str() {
            "--every-seconds" => {
                let value = cfg
                    .positional
                    .get(idx + 1)
                    .ok_or("refresh schedule add requires value after --every-seconds")?;
                let parsed = value
                    .parse::<i64>()
                    .map_err(|_| "refresh schedule add --every-seconds must be an integer")?;
                if parsed > 0 {
                    every_seconds = Some(parsed);
                }
                idx += 2;
            }
            "--tier" => {
                let value = cfg
                    .positional
                    .get(idx + 1)
                    .ok_or("refresh schedule add requires value after --tier")?;
                tier_seconds = Some(
                    tier_to_seconds(value)
                        .ok_or("refresh schedule add --tier must be one of: high, medium, low")?,
                );
                idx += 2;
            }
            "--urls" => {
                let value = cfg
                    .positional
                    .get(idx + 1)
                    .ok_or("refresh schedule add requires value after --urls")?;
                let parsed_urls = parse_urls_csv(value)?;
                urls = Some(parsed_urls);
                idx += 2;
            }
            token => {
                parse_seed_url_token(token, &mut seed_url)?;
                idx += 1;
            }
        }
    }

    let every_seconds = every_seconds
        .or(tier_seconds)
        .unwrap_or(REFRESH_TIER_MEDIUM_SECONDS);
    if seed_url.is_none() && urls.is_none() {
        return Err(
            anyhow::anyhow!("refresh schedule add requires [seed_url] or --urls <csv>").into(),
        );
    }
    Ok(RefreshScheduleAddInput {
        name,
        seed_url,
        urls,
        every_seconds,
    })
}

fn parse_urls_csv(value: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let parsed_urls = value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if parsed_urls.is_empty() {
        return Err(anyhow::anyhow!("refresh schedule add --urls cannot be empty").into());
    }
    for url in &parsed_urls {
        validate_url(url)?;
    }
    Ok(parsed_urls)
}

fn parse_seed_url_token(token: &str, seed_url: &mut Option<String>) -> Result<(), Box<dyn Error>> {
    if token.starts_with("--") {
        return Err(anyhow::anyhow!("unknown refresh schedule add flag: {token}").into());
    }
    if seed_url.is_some() {
        return Err(anyhow::anyhow!("refresh schedule add accepts at most one [seed_url]").into());
    }
    validate_url(token)?;
    *seed_url = Some(token.to_string());
    Ok(())
}

fn build_refresh_schedule_create(
    input: &RefreshScheduleAddInput,
) -> refresh_service::RefreshScheduleCreate {
    let next_run_at = Utc::now() + Duration::seconds(input.every_seconds);
    refresh_service::RefreshScheduleCreate {
        name: input.name.clone(),
        seed_url: input.seed_url.clone(),
        urls: input.urls.clone(),
        every_seconds: input.every_seconds,
        enabled: true,
        next_run_at,
        source_type: None,
        target: None,
    }
}

async fn create_refresh_watch_def(
    cfg: &Config,
    created: &crate::crates::services::refresh::RefreshSchedule,
) -> Result<(), Box<dyn Error>> {
    let watch_payload = serde_json::json!({
        "seed_url": created.seed_url,
        "urls": created.urls_json,
    });
    create_watch_def(
        cfg,
        &WatchDefCreate {
            name: created.name.clone(),
            task_type: "refresh".to_string(),
            task_payload: watch_payload,
            every_seconds: created.every_seconds,
            enabled: created.enabled,
            next_run_at: created.next_run_at,
        },
    )
    .await?;
    Ok(())
}

fn print_refresh_schedule_add_result(
    cfg: &Config,
    created: &crate::crates::services::refresh::RefreshSchedule,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&created)?);
    } else {
        println!(
            "{} created refresh schedule {}",
            symbol_for_status("completed"),
            accent(&created.name)
        );
        println!("  {} {}", muted("Every:"), created.every_seconds);
        println!("  {} {}", muted("Enabled:"), created.enabled);
    }
    Ok(())
}
