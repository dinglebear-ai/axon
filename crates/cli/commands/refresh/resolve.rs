use super::super::common::parse_urls;
use crate::crates::core::config::Config;
use crate::crates::core::content::url_to_domain;
use crate::crates::core::http::validate_url;
use crate::crates::crawl::manifest::read_manifest_urls;
use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;

fn manifest_candidate_paths(cfg: &Config, seed_url: &str) -> Vec<PathBuf> {
    let domain = url_to_domain(seed_url);
    let base = cfg.output_dir.join("domains").join(domain);
    vec![
        base.join("latest").join("manifest.jsonl"),
        base.join("sync").join("manifest.jsonl"),
    ]
}

pub async fn urls_from_manifest_seed(
    cfg: &Config,
    seed_url: &str,
) -> Result<Vec<String>, Box<dyn Error>> {
    for path in manifest_candidate_paths(cfg, seed_url) {
        if !path.exists() {
            continue;
        }
        let urls = read_manifest_urls(&path).await?;
        if !urls.is_empty() {
            let mut sorted: Vec<String> = urls.into_iter().collect();
            sorted.sort();
            return Ok(sorted);
        }
    }
    Ok(Vec::new())
}

pub async fn resolve_schedule_urls(
    cfg: &Config,
    schedule: &crate::crates::services::refresh::RefreshSchedule,
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut urls = match schedule.urls_json.as_ref() {
        Some(value) => serde_json::from_value::<Vec<String>>(value.clone()).unwrap_or_default(),
        None => Vec::new(),
    };

    if urls.is_empty()
        && let Some(seed_url) = schedule.seed_url.as_deref()
    {
        urls = urls_from_manifest_seed(cfg, seed_url).await?;
    }

    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    for url in urls {
        validate_url(&url)?;
        if seen.insert(url.clone()) {
            deduped.push(url);
        }
    }

    Ok(deduped)
}

fn looks_like_domain_seed(url: &str) -> bool {
    let Ok(parsed) = spider::url::Url::parse(url) else {
        return false;
    };
    parsed.path() == "/" && parsed.query().is_none() && parsed.fragment().is_none()
}

pub async fn resolve_refresh_urls(cfg: &Config) -> Result<Vec<String>, Box<dyn Error>> {
    let mut urls = parse_urls(cfg);

    if urls.is_empty() && !cfg.start_url.trim().is_empty() {
        let seeded = urls_from_manifest_seed(cfg, &cfg.start_url).await?;
        if !seeded.is_empty() {
            urls = seeded;
        }
    } else if urls.len() == 1 && looks_like_domain_seed(&urls[0]) {
        let seeded = urls_from_manifest_seed(cfg, &urls[0]).await?;
        if !seeded.is_empty() {
            urls = seeded;
        }
    }

    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    for url in urls {
        validate_url(&url)?;
        if seen.insert(url.clone()) {
            deduped.push(url);
        }
    }

    Ok(deduped)
}
