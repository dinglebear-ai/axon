use crate::crates::cli::commands::job_contracts::{
    JobCancelResponse, JobErrorsResponse, JobStatusResponse, JobSummaryEntry,
};
use crate::crates::core::config::{CommandKind, Config};
use crate::crates::core::http::normalize_url;
use crate::crates::core::logging::log_warn;
use crate::crates::core::ui::{accent, muted, primary, status_text, symbol_for_status};
use crate::crates::services::types::ServiceTimeRange;
use std::collections::HashSet;

/// Convert a CLI time-range string to the services-layer [`ServiceTimeRange`] enum.
///
/// Shared by `search` and `research` commands.
pub fn parse_service_time_range(value: Option<&str>) -> Option<ServiceTimeRange> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some("day") => Some(ServiceTimeRange::Day),
        Some("week") => Some(ServiceTimeRange::Week),
        Some("month") => Some(ServiceTimeRange::Month),
        Some("year") => Some(ServiceTimeRange::Year),
        _ => None,
    }
}

/// Truncate a string to at most `max_chars` characters, slicing on a char
/// boundary so multi-byte UTF-8 sequences never panic.
pub fn truncate_chars(s: &str, max_chars: usize) -> &str {
    s.char_indices().nth(max_chars).map_or(s, |(i, _)| &s[..i])
}

fn expand_numeric_range(start: i64, end: i64, step: i64) -> Vec<String> {
    let mut out = Vec::new();
    if step == 0 {
        return out;
    }
    let mut current = start;
    if start <= end && step > 0 {
        while current <= end {
            out.push(current.to_string());
            current += step;
        }
    } else if start >= end && step < 0 {
        while current >= end {
            out.push(current.to_string());
            current += step;
        }
    }
    out
}

fn expand_numeric_range_limited(
    start: i64,
    end: i64,
    step: i64,
    limit: usize,
) -> (Vec<String>, bool) {
    let mut values = expand_numeric_range(start, end, step);
    let truncated = values.len() > limit;
    if truncated {
        values.truncate(limit);
    }
    (values, truncated)
}

fn expand_brace_token(token: &str, limit: usize) -> (Vec<String>, bool) {
    let trimmed = token.trim();
    if let Some((lhs, rhs)) = trimmed.split_once("..") {
        let lhs = lhs.trim();
        let rhs = rhs.trim();
        if let (Ok(start), Ok(end)) = (lhs.parse::<i64>(), rhs.parse::<i64>()) {
            let step = if start <= end { 1 } else { -1 };
            let (values, truncated) = expand_numeric_range_limited(start, end, step, limit);
            if !values.is_empty() {
                return (values, truncated);
            }
        }
    }
    let mut values: Vec<String> = trimmed
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    let truncated = values.len() > limit;
    if truncated {
        values.truncate(limit);
    }
    (values, truncated)
}

const MAX_EXPANSION_DEPTH: usize = 10;
const MAX_EXPANSION_TOTAL: usize = 10_000;

fn expand_url_glob_seed(seed: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut warned = false;
    expand_url_glob_seed_inner(seed, 0, &mut out, &mut warned);
    out
}

fn expand_url_glob_seed_inner(seed: &str, depth: usize, out: &mut Vec<String>, warned: &mut bool) {
    if out.len() >= MAX_EXPANSION_TOTAL {
        if !*warned {
            log_warn(&format!(
                "URL glob expansion reached MAX_EXPANSION_TOTAL ({MAX_EXPANSION_TOTAL}) for seed: {seed}. Truncating."
            ));
            *warned = true;
        }
        return;
    }
    if depth >= MAX_EXPANSION_DEPTH {
        log_warn(&format!(
            "URL glob expansion reached MAX_EXPANSION_DEPTH ({MAX_EXPANSION_DEPTH}) for seed: {seed}. Truncating."
        ));
        out.push(seed.to_string());
        return;
    }
    let Some(open_idx) = seed.find('{') else {
        out.push(seed.to_string());
        return;
    };
    let Some(close_rel) = seed[open_idx + 1..].find('}') else {
        out.push(seed.to_string());
        return;
    };
    let close_idx = open_idx + 1 + close_rel;
    let prefix = &seed[..open_idx];
    let token = &seed[open_idx + 1..close_idx];
    let suffix = &seed[close_idx + 1..];
    let remaining = MAX_EXPANSION_TOTAL.saturating_sub(out.len());
    let (choices, truncated) = expand_brace_token(token, remaining);
    if truncated && !*warned {
        log_warn(&format!(
            "URL glob expansion reached MAX_EXPANSION_TOTAL ({MAX_EXPANSION_TOTAL}) for seed: {seed}. Truncating."
        ));
        *warned = true;
    }
    if choices.is_empty() {
        out.push(seed.to_string());
        return;
    }

    for choice in choices {
        let next = format!("{prefix}{choice}{suffix}");
        expand_url_glob_seed_inner(&next, depth + 1, out, warned);
        if out.len() >= MAX_EXPANSION_TOTAL {
            break;
        }
    }
}

pub fn parse_urls(cfg: &Config) -> Vec<String> {
    let mut out = Vec::new();
    let mut raw = Vec::new();
    if let Some(csv) = &cfg.urls_csv {
        raw.extend(
            csv.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        );
    }
    raw.extend(
        cfg.url_glob
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    );
    raw.extend(
        cfg.positional
            .iter()
            .flat_map(|s| s.split(','))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    );
    let mut seen = HashSet::new();
    for seed in raw {
        for expanded in expand_url_glob_seed(&seed) {
            let normalized = normalize_url(&expanded);
            if seen.insert(normalized.clone()) {
                out.push(normalized);
            }
        }
    }
    out
}

pub fn start_url_from_cfg(cfg: &Config) -> String {
    if cfg
        .positional
        .first()
        .is_some_and(|token| is_guarded_start_url_subcommand(cfg.command, token))
    {
        return cfg.start_url.clone();
    }

    if matches!(
        cfg.command,
        CommandKind::Scrape
            | CommandKind::Map
            | CommandKind::Crawl
            | CommandKind::Refresh
            | CommandKind::Extract
            | CommandKind::Embed
            | CommandKind::Screenshot
    ) {
        let selected = cfg
            .positional
            .first()
            .cloned()
            .unwrap_or_else(|| cfg.start_url.clone());
        return normalize_url(&selected);
    }

    cfg.start_url.clone()
}

fn is_guarded_start_url_subcommand(command: CommandKind, token: &str) -> bool {
    match command {
        CommandKind::Crawl => matches!(
            token,
            "status"
                | "cancel"
                | "errors"
                | "list"
                | "cleanup"
                | "clear"
                | "worker"
                | "recover"
                | "audit"
                | "diff"
        ),
        CommandKind::Refresh => matches!(
            token,
            "schedule"
                | "status"
                | "cancel"
                | "errors"
                | "list"
                | "cleanup"
                | "clear"
                | "worker"
                | "recover"
        ),
        CommandKind::Extract | CommandKind::Embed => matches!(
            token,
            "status" | "cancel" | "errors" | "list" | "cleanup" | "clear" | "worker" | "recover"
        ),
        _ => false,
    }
}

pub trait JobStatus {
    fn id(&self) -> uuid::Uuid;
    fn status(&self) -> &str;
    fn created_at(&self) -> chrono::DateTime<chrono::Utc>;
    fn updated_at(&self) -> chrono::DateTime<chrono::Utc>;
    fn error_text(&self) -> Option<&str>;
    fn to_status_response_json(&self) -> serde_json::Value;
    fn to_summary_entry_json(&self) -> serde_json::Value;
    fn to_errors_response_json(&self) -> serde_json::Value;
}

macro_rules! impl_job_status {
    ($ty:path, $status_ctor:path, $summary_ctor:path) => {
        impl JobStatus for $ty {
            fn id(&self) -> uuid::Uuid {
                self.id
            }
            fn status(&self) -> &str {
                &self.status
            }
            fn created_at(&self) -> chrono::DateTime<chrono::Utc> {
                self.created_at
            }
            fn updated_at(&self) -> chrono::DateTime<chrono::Utc> {
                self.updated_at
            }
            fn error_text(&self) -> Option<&str> {
                self.error_text.as_deref()
            }
            fn to_status_response_json(&self) -> serde_json::Value {
                serde_json::to_value($status_ctor(self)).unwrap_or_default()
            }
            fn to_summary_entry_json(&self) -> serde_json::Value {
                serde_json::to_value($summary_ctor(self)).unwrap_or_default()
            }
            fn to_errors_response_json(&self) -> serde_json::Value {
                serde_json::to_value(JobErrorsResponse::from_job(
                    self.id,
                    self.status.clone(),
                    self.error_text.clone(),
                ))
                .unwrap_or_default()
            }
        }
    };
}

impl_job_status!(
    crate::crates::jobs::crawl::CrawlJob,
    JobStatusResponse::from_crawl,
    JobSummaryEntry::from_crawl
);
impl_job_status!(
    crate::crates::jobs::extract::ExtractJob,
    JobStatusResponse::from_extract,
    JobSummaryEntry::from_extract
);
impl_job_status!(
    crate::crates::jobs::ingest::IngestJob,
    JobStatusResponse::from_ingest,
    JobSummaryEntry::from_ingest
);
impl_job_status!(
    crate::crates::jobs::embed::EmbedJob,
    JobStatusResponse::from_embed,
    JobSummaryEntry::from_embed
);
impl_job_status!(
    crate::crates::jobs::refresh::RefreshJob,
    JobStatusResponse::from_refresh,
    JobSummaryEntry::from_refresh
);

pub fn handle_job_status<T: JobStatus + serde::Serialize>(
    cfg: &Config,
    job: Option<T>,
    job_id: uuid::Uuid,
    command_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match job {
        Some(job) => {
            if cfg.json_output {
                let json = job.to_status_response_json();
                println!("{}", serde_json::to_string_pretty(&json)?);
            } else {
                println!(
                    "{} {}",
                    primary(&format!("{command_name} Status for")),
                    accent(&job.id().to_string())
                );
                println!(
                    "  {} {}",
                    symbol_for_status(job.status()),
                    status_text(job.status())
                );
                println!("  {} {}", muted("Created:"), job.created_at());
                println!("  {} {}", muted("Updated:"), job.updated_at());
                if let Some(err) = job.error_text() {
                    println!("  {} {}", muted("Error:"), err);
                }
                println!("Job ID: {}", job.id());
            }
        }
        None => {
            if cfg.json_output {
                println!(
                    "{}",
                    serde_json::json!({
                        "error": format!("job not found: {job_id}"),
                        "job_id": job_id
                    })
                );
            } else {
                println!(
                    "{} {}",
                    symbol_for_status("error"),
                    muted(&format!("job not found: {job_id}"))
                );
            }
        }
    }
    Ok(())
}

pub fn handle_job_cancel(
    cfg: &Config,
    id: uuid::Uuid,
    canceled: bool,
    command_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if cfg.json_output {
        let resp = JobCancelResponse::new(id, canceled);
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else if canceled {
        println!(
            "{} canceled {command_name} job {}",
            symbol_for_status("canceled"),
            accent(&id.to_string())
        );
        println!("Job ID: {id}");
    } else {
        println!(
            "{} no cancellable {command_name} job found for {}",
            symbol_for_status("error"),
            accent(&id.to_string())
        );
        println!("Job ID: {id}");
    }
    Ok(())
}

pub fn handle_job_errors<T: JobStatus + serde::Serialize>(
    cfg: &Config,
    job: Option<T>,
    id: uuid::Uuid,
    command_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match job {
        Some(job) => {
            if cfg.json_output {
                let contract = job.to_errors_response_json();
                println!("{}", serde_json::to_string_pretty(&contract)?);
            } else {
                println!(
                    "{} {} job {} {}",
                    symbol_for_status(job.status()),
                    command_name,
                    accent(&id.to_string()),
                    status_text(job.status())
                );
                println!(
                    "  {} {}",
                    muted("Error:"),
                    job.error_text().unwrap_or("None")
                );
                println!("Job ID: {id}");
            }
        }
        None => {
            if cfg.json_output {
                println!(
                    "{}",
                    serde_json::json!({
                        "error": format!("job not found: {id}"),
                        "job_id": id
                    })
                );
            } else {
                println!(
                    "{} {}",
                    symbol_for_status("error"),
                    muted(&format!("job not found: {id}"))
                );
            }
        }
    }
    Ok(())
}

pub fn handle_job_list<T: JobStatus + serde::Serialize>(
    cfg: &Config,
    jobs: Vec<T>,
    command_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if cfg.json_output {
        let entries: Vec<serde_json::Value> =
            jobs.iter().map(|j| j.to_summary_entry_json()).collect();
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }

    println!("{}", primary(&format!("{command_name} Jobs")));
    if jobs.is_empty() {
        println!("  {}", muted(&format!("No {command_name} jobs found.")));
        return Ok(());
    }

    for job in jobs {
        println!(
            "  {} {} {}",
            symbol_for_status(job.status()),
            accent(&job.id().to_string()),
            status_text(job.status())
        );
    }
    Ok(())
}

pub fn handle_job_cleanup(
    cfg: &Config,
    removed: u64,
    command_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if cfg.json_output {
        println!("{}", serde_json::json!({ "removed": removed }));
    } else {
        println!(
            "{} removed {} {command_name} jobs",
            symbol_for_status("completed"),
            removed
        );
    }
    Ok(())
}

pub fn handle_job_clear(
    cfg: &Config,
    removed: u64,
    command_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({ "removed": removed, "queue_purged": true })
        );
    } else {
        println!(
            "{} cleared {} {command_name} jobs and purged queue",
            symbol_for_status("completed"),
            removed
        );
    }
    Ok(())
}

pub fn handle_job_recover(
    cfg: &Config,
    reclaimed: u64,
    command_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if cfg.json_output {
        println!("{}", serde_json::json!({ "reclaimed": reclaimed }));
    } else {
        println!(
            "{} reclaimed {} stale {command_name} jobs",
            symbol_for_status("completed"),
            reclaimed
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        JobStatus, MAX_EXPANSION_TOTAL, expand_url_glob_seed, start_url_from_cfg, truncate_chars,
    };
    use crate::crates::core::config::{CommandKind, Config};
    use crate::crates::jobs::embed::EmbedJob;
    use crate::crates::jobs::refresh::RefreshJob;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    #[test]
    fn truncate_chars_multibyte() {
        // ASCII — no truncation needed
        assert_eq!(truncate_chars("hello", 5), "hello");
        // ASCII — truncation
        assert_eq!(truncate_chars("hello", 3), "hel");
        // Multi-byte char boundary
        assert_eq!(truncate_chars("héllo", 3), "hél");
        // Zero limit
        assert_eq!(truncate_chars("hello", 0), "");
        // Limit exceeds length
        assert_eq!(truncate_chars("hi", 10), "hi");
    }

    #[test]
    fn expands_url_glob_range() {
        let expanded = expand_url_glob_seed("https://example.com/page/{1..3}");
        assert_eq!(
            expanded,
            vec![
                "https://example.com/page/1".to_string(),
                "https://example.com/page/2".to_string(),
                "https://example.com/page/3".to_string()
            ]
        );
    }

    #[test]
    fn expands_url_glob_list_and_nested() {
        let expanded = expand_url_glob_seed("https://example.com/{news,docs}/{a,b}");
        assert_eq!(
            expanded,
            vec![
                "https://example.com/news/a".to_string(),
                "https://example.com/news/b".to_string(),
                "https://example.com/docs/a".to_string(),
                "https://example.com/docs/b".to_string()
            ]
        );
    }

    #[test]
    fn expands_url_glob_with_total_cap() {
        let expanded = expand_url_glob_seed("https://example.com/page/{1..20000}");
        assert_eq!(expanded.len(), MAX_EXPANSION_TOTAL);
        assert_eq!(
            expanded.first().map(String::as_str),
            Some("https://example.com/page/1")
        );
        assert_eq!(
            expanded.last().map(String::as_str),
            Some("https://example.com/page/10000")
        );
    }

    #[test]
    fn start_url_from_cfg_guards_crawl_audit_tokens() {
        let cfg = Config {
            command: CommandKind::Crawl,
            start_url: "https://fallback.example".to_string(),
            positional: vec!["audit".to_string(), "https://target.example".to_string()],
            ..Config::default()
        };

        assert_eq!(start_url_from_cfg(&cfg), "https://fallback.example");
    }

    #[test]
    fn start_url_from_cfg_guards_refresh_schedule_tokens() {
        let cfg = Config {
            command: CommandKind::Refresh,
            start_url: "https://fallback.example".to_string(),
            positional: vec!["schedule".to_string(), "list".to_string()],
            ..Config::default()
        };

        assert_eq!(start_url_from_cfg(&cfg), "https://fallback.example");
    }

    fn test_ts() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 15, 12, 0, 0)
            .single()
            .expect("valid timestamp")
    }

    fn assert_job_status_trait<T: JobStatus>(job: &T, expected_status: &str) {
        assert_eq!(job.status(), expected_status);
        assert_eq!(job.updated_at(), test_ts());
    }

    #[test]
    fn embed_job_implements_shared_job_status_trait() {
        let job = EmbedJob {
            id: Uuid::parse_str("66666666-6666-6666-6666-666666666666").expect("valid uuid"),
            status: "running".to_string(),
            created_at: test_ts(),
            updated_at: test_ts(),
            started_at: Some(test_ts()),
            finished_at: None,
            error_text: None,
            input_text: "/tmp/embed-input".to_string(),
            result_json: Some(serde_json::json!({"chunks_embedded": 3})),
            config_json: serde_json::json!({"collection": "cortex"}),
        };

        assert_job_status_trait(&job, "running");
    }

    #[test]
    fn refresh_job_implements_shared_job_status_trait() {
        let job = RefreshJob {
            id: Uuid::parse_str("77777777-7777-7777-7777-777777777777").expect("valid uuid"),
            status: "completed".to_string(),
            created_at: test_ts(),
            updated_at: test_ts(),
            started_at: Some(test_ts()),
            finished_at: Some(test_ts()),
            error_text: None,
            urls_json: serde_json::json!(["https://example.com"]),
            result_json: Some(serde_json::json!({"checked": 1})),
            config_json: serde_json::json!({"embed": true}),
        };

        assert_job_status_trait(&job, "completed");
    }
}
