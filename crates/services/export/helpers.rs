use crate::crates::services::types::{
    ExportIntegrity, GithubSeedExport, QuerySeedExport, RebuildSeedsExport, ScrapeSeedExport,
};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub(super) fn build_integrity(seeds: &RebuildSeedsExport) -> ExportIntegrity {
    let mut counts = HashMap::new();
    let mut hashes = HashMap::new();

    counts.insert(
        "crawl_seed_urls".to_string(),
        seeds.crawl_seed_urls.len() as u64,
    );
    counts.insert("scrape_urls".to_string(), seeds.scrape_urls.len() as u64);
    counts.insert(
        "scrape_requests".to_string(),
        seeds.scrape_requests.len() as u64,
    );
    counts.insert("github_repos".to_string(), seeds.github_repos.len() as u64);
    counts.insert(
        "github_requests".to_string(),
        seeds.github_requests.len() as u64,
    );
    counts.insert(
        "reddit_targets".to_string(),
        seeds.reddit_targets.len() as u64,
    );
    counts.insert(
        "youtube_targets".to_string(),
        seeds.youtube_targets.len() as u64,
    );
    counts.insert(
        "session_targets".to_string(),
        seeds.session_targets.len() as u64,
    );
    counts.insert("local_paths".to_string(), seeds.local_paths.len() as u64);
    counts.insert(
        "extraction_requests".to_string(),
        seeds.extraction_requests.len() as u64,
    );
    counts.insert(
        "search_requests".to_string(),
        seeds.search_requests.len() as u64,
    );
    counts.insert(
        "research_requests".to_string(),
        seeds.research_requests.len() as u64,
    );

    hashes.insert(
        "crawl_seed_urls".to_string(),
        hash_sorted_strings(&seeds.crawl_seed_urls),
    );
    hashes.insert(
        "scrape_urls".to_string(),
        hash_sorted_strings(&seeds.scrape_urls),
    );
    hashes.insert(
        "github_repos".to_string(),
        hash_sorted_strings(&seeds.github_repos),
    );
    hashes.insert(
        "reddit_targets".to_string(),
        hash_sorted_strings(&seeds.reddit_targets),
    );
    hashes.insert(
        "youtube_targets".to_string(),
        hash_sorted_strings(&seeds.youtube_targets),
    );
    hashes.insert(
        "session_targets".to_string(),
        hash_sorted_strings(&seeds.session_targets),
    );
    hashes.insert(
        "local_paths".to_string(),
        hash_sorted_strings(&seeds.local_paths),
    );
    hashes.insert(
        "search_queries".to_string(),
        hash_sorted_strings(&seeds.search_queries),
    );
    hashes.insert(
        "research_queries".to_string(),
        hash_sorted_strings(&seeds.research_queries),
    );

    ExportIntegrity { counts, hashes }
}

fn hash_sorted_strings(values: &[String]) -> String {
    let mut sorted = values
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    sorted.sort();
    sorted.dedup();
    let payload = sorted.join("\n");
    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    hex::encode(hasher.finalize())
}

pub(super) fn dedup_sorted<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut out = values
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

pub(super) fn dedup_query_requests(requests: Vec<QuerySeedExport>) -> Vec<QuerySeedExport> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for req in requests {
        let key = (
            req.query.clone(),
            serde_json::to_string(&req.options).unwrap_or_else(|_| "{}".to_string()),
        );
        if seen.insert(key) {
            out.push(req);
        }
    }
    out
}

pub(super) fn dedup_scrape_requests(requests: Vec<ScrapeSeedExport>) -> Vec<ScrapeSeedExport> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for req in requests {
        let key = (
            req.url.clone(),
            serde_json::to_string(&req.options).unwrap_or_else(|_| "{}".to_string()),
        );
        if seen.insert(key) {
            out.push(req);
        }
    }
    out
}

pub(super) fn dedup_github_seed_requests(requests: Vec<GithubSeedExport>) -> Vec<GithubSeedExport> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for req in requests {
        let key = (
            req.target.clone(),
            serde_json::to_string(&req.options).unwrap_or_else(|_| "{}".to_string()),
        );
        if seen.insert(key) {
            out.push(req);
        }
    }
    out
}

pub(super) fn status_matches(status: &str, statuses: &[String]) -> bool {
    statuses.is_empty() || statuses.iter().any(|s| s == status)
}

pub(super) fn to_rfc3339_opt(ts: Option<DateTime<Utc>>) -> Option<String> {
    ts.map(|value| value.to_rfc3339())
}

pub(super) fn json_num_to_u64(value: Option<serde_json::Value>) -> Option<u64> {
    value.and_then(|v| {
        if v.is_null() {
            return None;
        }
        v.as_u64()
            .or_else(|| v.as_i64().and_then(|n| u64::try_from(n).ok()))
            .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
    })
}

pub(super) fn json_string_opt(value: Option<serde_json::Value>) -> Option<String> {
    value.and_then(|v| v.as_str().map(str::to_string))
}

pub(super) fn json_array_to_strings(value: Option<serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}
