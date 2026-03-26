use crate::crates::core::config::Config;
use crate::crates::core::ui::{accent, muted, symbol_for_status};
use crate::crates::services::refresh::{RefreshScheduleCreate, create_refresh_schedule};
use chrono::Utc;
use std::error::Error;

use super::schedule::tier_to_seconds;

const REFRESH_TIER_MEDIUM_SECONDS: i64 = 21600;

fn validate_github_repo(repo: &str) -> Result<&str, Box<dyn Error>> {
    let trimmed = repo.trim();
    let mut parts = trimmed.split('/');
    let Some(owner) = parts.next() else {
        return Err(anyhow::anyhow!("Invalid GitHub target. Expected owner/repo").into());
    };
    let Some(name) = parts.next() else {
        return Err(anyhow::anyhow!("Invalid GitHub target. Expected owner/repo").into());
    };
    if parts.next().is_some() || owner.is_empty() || name.is_empty() {
        return Err(anyhow::anyhow!("Invalid GitHub target. Expected owner/repo").into());
    }
    let valid_segment = |segment: &str| {
        segment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
            && !segment.starts_with('.')
            && !segment.ends_with('.')
    };
    if !valid_segment(owner) || !valid_segment(name) {
        return Err(anyhow::anyhow!("Invalid GitHub target. Expected owner/repo").into());
    }
    Ok(trimmed)
}

/// Create a GitHub repo re-ingest schedule.
///
/// Parses `--every-seconds` and `--tier` from remaining positional args (same as URL schedules).
/// The schedule name defaults to the repo slug with `/` replaced by `-`.
pub(crate) async fn handle_refresh_schedule_add_github(
    cfg: &Config,
    repo: &str,
    raw_name: &str,
) -> Result<(), Box<dyn Error>> {
    let repo = validate_github_repo(repo)?;
    let mut every_seconds: Option<i64> = None;
    let mut tier_seconds: Option<i64> = None;

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
            token => {
                return Err(
                    anyhow::anyhow!("unknown flag for github refresh schedule: {token}").into(),
                );
            }
        }
    }

    let every_seconds = every_seconds
        .or(tier_seconds)
        .unwrap_or(REFRESH_TIER_MEDIUM_SECONDS);
    let schedule_name = raw_name.replace('/', "-");
    let next_run_at = Utc::now();
    let schedule = RefreshScheduleCreate {
        name: schedule_name.clone(),
        seed_url: None,
        urls: None,
        every_seconds,
        enabled: true,
        next_run_at,
        source_type: Some("github".to_string()),
        target: Some(repo.to_string()),
    };
    let created = create_refresh_schedule(cfg, &schedule).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&created)?);
    } else {
        println!(
            "{} created github refresh schedule {}",
            symbol_for_status("completed"),
            accent(&created.name)
        );
        println!("  {} {}", muted("Repo:"), repo);
        println!("  {} {}", muted("Every:"), created.every_seconds);
        println!("  {} {}", muted("Enabled:"), created.enabled);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_github_repo;

    #[test]
    fn validate_github_repo_accepts_owner_repo_slug() {
        assert_eq!(
            validate_github_repo("owner/repo").expect("valid repo"),
            "owner/repo"
        );
    }

    #[test]
    fn validate_github_repo_rejects_non_slug_inputs() {
        assert!(validate_github_repo("https://github.com/owner/repo").is_err());
        assert!(validate_github_repo("owner").is_err());
        assert!(validate_github_repo("owner/repo/extra").is_err());
        assert!(validate_github_repo("../repo").is_err());
    }
}
