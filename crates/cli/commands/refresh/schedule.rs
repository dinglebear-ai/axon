mod add;
mod run_due;
mod worker;

use crate::crates::core::config::Config;
use crate::crates::core::ui::{accent, muted, subtle, symbol_for_status};
use crate::crates::jobs::watch::list_watch_defs;
use crate::crates::services::refresh as refresh_service;
use chrono::{DateTime, Utc};
use std::error::Error;

pub use run_due::handle_refresh_schedule_run_due;
#[cfg(test)]
#[allow(unused_imports)]
pub use worker::refresh_schedule_tick_secs_default;

pub(super) const REFRESH_TIER_HIGH_SECONDS: i64 = 1800;
pub(super) const REFRESH_TIER_MEDIUM_SECONDS: i64 = 21600;
pub(super) const REFRESH_TIER_LOW_SECONDS: i64 = 86400;

pub fn tier_to_seconds(tier: &str) -> Option<i64> {
    match tier.trim().to_ascii_lowercase().as_str() {
        "high" => Some(REFRESH_TIER_HIGH_SECONDS),
        "medium" => Some(REFRESH_TIER_MEDIUM_SECONDS),
        "low" => Some(REFRESH_TIER_LOW_SECONDS),
        _ => None,
    }
}

pub async fn handle_refresh_schedule(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let action = cfg
        .positional
        .get(1)
        .map(|s| s.as_str())
        .ok_or("refresh schedule requires a subcommand")?;
    match action {
        "add" => add::handle_refresh_schedule_add(cfg).await?,
        "list" => handle_refresh_schedule_list(cfg).await?,
        "enable" => handle_refresh_schedule_enable(cfg).await?,
        "disable" => handle_refresh_schedule_disable(cfg).await?,
        "delete" => handle_refresh_schedule_delete(cfg).await?,
        "run-due" => handle_refresh_schedule_run_due(cfg).await?,
        "worker" => worker::handle_refresh_schedule_worker(cfg).await?,
        _ => return Err(format!("unknown refresh schedule subcommand: {action}").into()),
    }
    Ok(())
}

pub(super) fn schedule_name_arg(
    cfg: &Config,
    action: &str,
    index: usize,
) -> Result<String, Box<dyn Error>> {
    cfg.positional
        .get(index)
        .cloned()
        .ok_or_else(|| format!("refresh schedule {action} requires <name>").into())
}

async fn handle_refresh_schedule_list(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let (watch_defs, legacy_schedules) = tokio::try_join!(
        list_watch_defs(cfg, 500),
        refresh_service::refresh_schedule_list(cfg, 200)
    )?;
    let mut seen = std::collections::HashSet::new();
    let mut schedules: Vec<serde_json::Value> = watch_defs
        .into_iter()
        .filter(|w| w.task_type == "refresh")
        .map(|w| {
            seen.insert(w.name.clone());
            let seed_url = w
                .task_payload
                .get("seed_url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            serde_json::json!({
                "name": w.name,
                "enabled": w.enabled,
                "every_seconds": w.every_seconds,
                "next_run_at": w.next_run_at.to_rfc3339(),
                "last_run_at": w.last_run_at.map(|t| t.to_rfc3339()),
                "seed_url": seed_url,
            })
        })
        .collect();
    for s in legacy_schedules {
        if !seen.contains(&s.name) {
            schedules.push(serde_json::json!({
                "name": s.name,
                "enabled": s.enabled,
                "every_seconds": s.every_seconds,
                "next_run_at": s.next_run_at.to_rfc3339(),
                "last_run_at": s.last_run_at.map(|t| t.to_rfc3339()),
                "seed_url": s.seed_url,
            }));
        }
    }
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&schedules)?);
        return Ok(());
    }

    println!("{}", crate::crates::core::ui::primary("Refresh Schedules"));
    if schedules.is_empty() {
        println!("  {}", muted("No refresh schedules found."));
        return Ok(());
    }

    let now = Utc::now();
    for schedule in schedules {
        let enabled = schedule
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let name = schedule
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        let every_secs = schedule
            .get("every_seconds")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let next_run_at = schedule
            .get("next_run_at")
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&Utc));
        let last_run_at = schedule
            .get("last_run_at")
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&Utc));
        let seed_url = schedule.get("seed_url").and_then(|v| v.as_str());

        let (symbol, state_label) = if enabled {
            (symbol_for_status("completed"), accent("active"))
        } else {
            (symbol_for_status("pending"), muted("paused"))
        };
        let every_str = format_every_seconds(every_secs);
        let next_str = next_run_at
            .map(|t| format_time_until(t, now))
            .unwrap_or_else(|| "unknown".to_string());
        let last_str = last_run_at
            .map(|t| format_time_ago(t, now))
            .unwrap_or_else(|| "never run".to_string());

        println!(
            "  {} {}  {}  every {}  next {}  {}",
            symbol,
            accent(name),
            state_label,
            muted(&every_str),
            muted(&next_str),
            muted(&last_str),
        );
        if let Some(seed) = seed_url {
            println!("    {} {}", muted("seed:"), subtle(seed));
        }
    }
    Ok(())
}

fn format_every_seconds(secs: i64) -> String {
    if secs <= 0 {
        return "?".to_string();
    }
    if secs >= 86400 && secs % 86400 == 0 {
        format!("{}d", secs / 86400)
    } else if secs >= 3600 && secs % 3600 == 0 {
        format!("{}h", secs / 3600)
    } else if secs >= 60 && secs % 60 == 0 {
        format!("{}m", secs / 60)
    } else {
        format!("{}s", secs)
    }
}

fn format_time_until(target: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let diff = (target - now).num_seconds();
    if diff <= 0 {
        return "overdue".to_string();
    }
    let h = diff / 3600;
    let m = (diff % 3600) / 60;
    if h > 0 {
        format!("in {}h {}m", h, m)
    } else if m > 0 {
        format!("in {}m", m)
    } else {
        "in <1m".to_string()
    }
}

fn format_time_ago(target: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let diff = (now - target).num_seconds();
    if diff < 0 {
        return "just now".to_string();
    }
    let h = diff / 3600;
    let m = (diff % 3600) / 60;
    if h > 0 {
        format!("{}h {}m ago", h, m)
    } else if m > 0 {
        format!("{}m ago", m)
    } else {
        "just now".to_string()
    }
}

async fn handle_refresh_schedule_enable(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let name = schedule_name_arg(cfg, "enable", 2)?;
    let updated = refresh_service::refresh_schedule_enable(cfg, &name).await?;
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({"name": name, "enabled": true, "updated": updated})
        );
    } else if updated {
        println!(
            "{} enabled refresh schedule {}",
            symbol_for_status("completed"),
            accent(&name)
        );
    } else {
        println!(
            "{} refresh schedule not found: {}",
            symbol_for_status("error"),
            accent(&name)
        );
    }
    Ok(())
}

async fn handle_refresh_schedule_disable(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let name = schedule_name_arg(cfg, "disable", 2)?;
    let updated = refresh_service::refresh_schedule_disable(cfg, &name).await?;
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({"name": name, "enabled": false, "updated": updated})
        );
    } else if updated {
        println!(
            "{} disabled refresh schedule {}",
            symbol_for_status("completed"),
            accent(&name)
        );
    } else {
        println!(
            "{} refresh schedule not found: {}",
            symbol_for_status("error"),
            accent(&name)
        );
    }
    Ok(())
}

async fn handle_refresh_schedule_delete(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let name = schedule_name_arg(cfg, "delete", 2)?;
    let deleted = refresh_service::refresh_schedule_delete(cfg, &name).await?;
    if cfg.json_output {
        println!("{}", serde_json::json!({"name": name, "deleted": deleted}));
    } else if deleted {
        println!(
            "{} deleted refresh schedule {}",
            symbol_for_status("completed"),
            accent(&name)
        );
    } else {
        println!(
            "{} refresh schedule not found: {}",
            symbol_for_status("error"),
            accent(&name)
        );
    }
    Ok(())
}

#[cfg(test)]
mod format_tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn format_every_seconds_zero_returns_sentinel() {
        assert_eq!(format_every_seconds(0), "?");
    }

    #[test]
    fn format_every_seconds_negative_returns_sentinel() {
        assert_eq!(format_every_seconds(-1), "?");
    }

    #[test]
    fn format_every_seconds_picks_largest_clean_unit() {
        assert_eq!(format_every_seconds(86400), "1d");
        assert_eq!(format_every_seconds(21600), "6h");
        assert_eq!(format_every_seconds(1800), "30m");
        assert_eq!(format_every_seconds(45), "45s");
    }

    #[test]
    fn format_time_until_overdue_when_past() {
        let now = Utc::now();
        assert_eq!(
            format_time_until(now - Duration::seconds(1), now),
            "overdue"
        );
    }

    #[test]
    fn format_time_until_sub_minute() {
        let now = Utc::now();
        assert_eq!(
            format_time_until(now + Duration::seconds(30), now),
            "in <1m"
        );
    }

    #[test]
    fn format_time_until_minutes_only() {
        let now = Utc::now();
        assert_eq!(
            format_time_until(now + Duration::minutes(45), now),
            "in 45m"
        );
    }

    #[test]
    fn format_time_until_hours_and_minutes() {
        let now = Utc::now();
        assert_eq!(
            format_time_until(now + Duration::seconds(5 * 3600 + 54 * 60), now),
            "in 5h 54m"
        );
    }

    #[test]
    fn format_time_ago_negative_diff_returns_just_now() {
        let now = Utc::now();
        assert_eq!(
            format_time_ago(now + Duration::seconds(10), now),
            "just now"
        );
    }

    #[test]
    fn format_time_ago_sub_minute_returns_just_now() {
        let now = Utc::now();
        assert_eq!(
            format_time_ago(now - Duration::seconds(30), now),
            "just now"
        );
    }

    #[test]
    fn format_time_ago_hours_and_minutes() {
        let now = Utc::now();
        assert_eq!(
            format_time_ago(now - Duration::seconds(2 * 3600 + 15 * 60), now),
            "2h 15m ago"
        );
    }
}
