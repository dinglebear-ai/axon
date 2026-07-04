use axon_core::config::{CommandKind, Config};
use axon_core::http::normalize_url;
use axon_core::logging::log_warn;
use std::collections::HashSet;

/// Truncate a string to at most `max_chars` characters, slicing on a char
/// boundary so multi-byte UTF-8 sequences never panic.
pub fn truncate_chars(s: &str, max_chars: usize) -> &str {
    s.char_indices().nth(max_chars).map_or(s, |(i, _)| &s[..i])
}

fn expand_numeric_range_limited(
    start: i64,
    end: i64,
    step: i64,
    limit: usize,
) -> (Vec<String>, bool) {
    let mut out = Vec::new();
    if step == 0 {
        return (out, false);
    }
    let mut current = start;
    let mut truncated = false;
    if start <= end && step > 0 {
        while current <= end {
            if out.len() >= limit {
                truncated = true;
                break;
            }
            out.push(current.to_string());
            let Some(next) = current.checked_add(step) else {
                break;
            };
            current = next;
        }
    } else if start >= end && step < 0 {
        while current >= end {
            if out.len() >= limit {
                truncated = true;
                break;
            }
            out.push(current.to_string());
            let Some(next) = current.checked_add(step) else {
                break;
            };
            current = next;
        }
    }
    (out, truncated)
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
            let normalized = normalize_url(&expanded).into_owned();
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
        CommandKind::Map | CommandKind::Extract | CommandKind::Screenshot
    ) {
        let selected = cfg
            .positional
            .first()
            .cloned()
            .unwrap_or_else(|| cfg.start_url.clone());
        return normalize_url(&selected).into_owned();
    }

    cfg.start_url.clone()
}

fn is_guarded_start_url_subcommand(command: CommandKind, token: &str) -> bool {
    match command {
        CommandKind::Extract => matches!(
            token,
            "status" | "cancel" | "errors" | "list" | "cleanup" | "clear" | "worker" | "recover"
        ),
        _ => false,
    }
}

#[cfg(test)]
#[path = "common_urls_tests.rs"]
mod tests;
