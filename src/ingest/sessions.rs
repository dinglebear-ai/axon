use crate::core::config::Config;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::ingest::progress::PhaseReporter;
use crate::vector::ops::{PreparedDoc, embed_prepared_docs};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;

const PHASE_SCANNING: &str = "scanning_sessions";
const PHASE_EMBEDDING: &str = "embedding_sessions";
const SESSION_INGEST_MAX_BYTES_ENV: &str = "AXON_SESSION_INGEST_MAX_BYTES";
const DEFAULT_SESSION_INGEST_MAX_BYTES: u64 = 20 * 1024 * 1024;

mod claude;
mod codex;
mod gemini;

pub(crate) type IngestResult<T> = Result<T, anyhow::Error>;

/// A parsed session document ready for embedding.
pub(crate) struct SessionDoc {
    pub(crate) doc: PreparedDoc,
    pub(crate) collection: String,
}

pub(crate) fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

pub(crate) async fn read_session_file_limited(path: &Path) -> IngestResult<String> {
    let max_bytes = session_ingest_max_bytes();
    let meta = fs::metadata(path).await?;
    if meta.len() > max_bytes {
        anyhow::bail!(
            "session file exceeds AXON_SESSION_INGEST_MAX_BYTES limit: {} > {} bytes ({})",
            meta.len(),
            max_bytes,
            path.display()
        );
    }
    Ok(fs::read_to_string(path).await?)
}

fn session_ingest_max_bytes() -> u64 {
    std::env::var(SESSION_INGEST_MAX_BYTES_ENV)
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_SESSION_INGEST_MAX_BYTES)
}

pub(crate) fn redact_session_text(input: &str) -> String {
    input
        .split_whitespace()
        .map(redact_session_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_session_token(token: &str) -> String {
    let trimmed = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-');
    let lower = trimmed.to_ascii_lowercase();
    let secret_like = lower.starts_with("sk-")
        || lower.starts_with("ghp_")
        || lower.starts_with("github_pat_")
        || lower.starts_with("atk_")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("access_token")
        || (trimmed.len() >= 24
            && trimmed.chars().any(|c| c.is_ascii_alphabetic())
            && trimmed.chars().any(|c| c.is_ascii_digit()));
    if secret_like {
        token.replace(trimmed, "[redacted-secret]")
    } else {
        token.to_string()
    }
}

pub async fn ingest_sessions(
    cfg: &Config,
    reporter: &PhaseReporter,
) -> Result<usize, Box<dyn Error>> {
    log_info("command=ingest source=sessions");
    reporter.report_phase(PHASE_SCANNING).await;

    let multi = MultiProgress::new();
    let main_pb = multi.add(ProgressBar::new_spinner());
    main_pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .unwrap(),
    );
    main_pb.set_message("Scanning session files...");
    main_pb.enable_steady_tick(Duration::from_millis(100));

    let all_platforms = !cfg.sessions_claude && !cfg.sessions_codex && !cfg.sessions_gemini;
    let mut all_docs: Vec<SessionDoc> = Vec::new();

    if cfg.sessions_claude || all_platforms {
        let docs = claude::collect_claude_docs(cfg, &multi)
            .await
            .unwrap_or_default();
        log_info(&format!("sessions platform=claude files={}", docs.len()));
        all_docs.extend(docs);
    }
    if cfg.sessions_codex || all_platforms {
        let docs = codex::collect_codex_docs(cfg, &multi)
            .await
            .unwrap_or_default();
        log_info(&format!("sessions platform=codex files={}", docs.len()));
        all_docs.extend(docs);
    }
    if cfg.sessions_gemini || all_platforms {
        let docs = gemini::collect_gemini_docs(cfg, &multi)
            .await
            .unwrap_or_default();
        log_info(&format!("sessions platform=gemini files={}", docs.len()));
        all_docs.extend(docs);
    }

    reporter.report_phase(PHASE_EMBEDDING).await;
    main_pb.set_message(format!("Embedding {} session files...", all_docs.len()));

    let total_chunks = embed_all_session_docs(cfg, all_docs).await;

    main_pb.finish_with_message(format!("Done: {} chunks embedded", total_chunks));
    log_done(&format!(
        "command=ingest source=sessions total_chunk_count={total_chunks}"
    ));
    Ok(total_chunks)
}

/// Groups collected docs by collection and calls `embed_prepared_docs` once per collection.
async fn embed_all_session_docs(cfg: &Config, docs: Vec<SessionDoc>) -> usize {
    let mut by_collection: HashMap<String, Vec<PreparedDoc>> = HashMap::new();
    for sd in docs {
        by_collection.entry(sd.collection).or_default().push(sd.doc);
    }

    let mut total = 0;
    for (collection, prepared) in by_collection {
        let mut session_cfg = cfg.clone();
        session_cfg.collection = collection;

        match embed_prepared_docs(&session_cfg, prepared, None).await {
            Ok(summary) => {
                total += summary.chunks_embedded;
                if summary.docs_failed > 0 {
                    log_warn(&format!(
                        "sessions embed partial failure collection={} docs_failed={} docs_embedded={}",
                        session_cfg.collection, summary.docs_failed, summary.docs_embedded
                    ));
                }
            }
            Err(e) => {
                log_warn(&format!(
                    "sessions embed failed collection={} error={e}",
                    session_cfg.collection
                ));
            }
        }
    }
    total
}

pub(super) fn flatten_session_result(
    res: Result<IngestResult<Option<SessionDoc>>, tokio::task::JoinError>,
    label: &str,
) -> Option<SessionDoc> {
    match res {
        Ok(Ok(opt)) => opt,
        Ok(Err(e)) => {
            log_warn(&format!("{label}: {e}"));
            None
        }
        Err(join_err) => {
            log_warn(&format!("{label} task failed: {join_err}"));
            None
        }
    }
}

pub(crate) fn resolve_collection(cfg: &Config, derived_name: &str) -> String {
    if cfg.collection != "axon" {
        return cfg.collection.clone();
    }
    if derived_name.is_empty() {
        return "global-sessions".to_string();
    }
    format!("{}-sessions", derived_name)
}

pub(crate) fn matches_project_filter(cfg: &Config, name: &str) -> bool {
    if let Some(filter) = &cfg.sessions_project {
        name.to_lowercase().contains(&filter.to_lowercase())
    } else {
        true
    }
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;

/// Session-level metadata collected once per project directory, injected into
/// every `PreparedDoc.extra` produced by that project's session files.
#[derive(Clone)]
pub(crate) struct SessionMeta {
    pub(crate) agent: &'static str,
    pub(crate) project_name: String,
    pub(crate) project_path: Option<String>,
    pub(crate) gh_repo: Option<String>,
}

/// Decode a Claude project directory name back to the actual filesystem path.
///
/// Claude encodes project paths by replacing `/` with `-` and `_` with `-`,
/// and literal `-` with `--`. Because `_` and path separators both become `-`,
/// the decode is lossy and requires greedy filesystem probing to resolve
/// ambiguities.
///
/// Example: `-home-jmagar-workspace-axon-rust` → `/home/jmagar/workspace/axon_rust`
pub(crate) fn decode_claude_project_path(dir_name: &str) -> Option<PathBuf> {
    let without_prefix = dir_name.trim_start_matches('-');
    if without_prefix.is_empty() {
        return None;
    }
    // `--` encodes a literal `-` in a component; substitute before splitting on `-`
    let with_placeholder = without_prefix.replace("--", "\x01");
    let parts: Vec<String> = with_placeholder
        .split('-')
        .filter(|s| !s.is_empty())
        .map(|s| s.replace('\x01', "-"))
        .collect();
    if parts.is_empty() {
        return None;
    }
    decode_path_walk(Path::new("/"), &parts, 0)
}

/// Greedy filesystem-probing walk. Tries consuming 1..n dash-joined parts as a
/// single directory segment, testing both the dash form and the underscore form.
fn decode_path_walk(current: &Path, parts: &[String], start: usize) -> Option<PathBuf> {
    if start >= parts.len() {
        return if current.is_dir() {
            Some(current.to_path_buf())
        } else {
            None
        };
    }
    for n in 1..=(parts.len() - start) {
        let segment = parts[start..start + n].join("-");
        let candidate = current.join(&segment);
        if candidate.is_dir()
            && let Some(result) = decode_path_walk(&candidate, parts, start + n)
        {
            return Some(result);
        }
        // Also try underscores in place of dashes (handles `axon_rust` encoded as `axon-rust`)
        let segment_us = segment.replace('-', "_");
        if segment_us != segment {
            let candidate_us = current.join(&segment_us);
            if candidate_us.is_dir()
                && let Some(result) = decode_path_walk(&candidate_us, parts, start + n)
            {
                return Some(result);
            }
        }
    }
    None
}

/// Read `remote.origin.url` from `.git/config` at the given project directory.
///
/// Returns the normalized `"owner/repo"` slug extracted from the remote URL.
/// Handles both HTTPS (`https://[token@]github.com/owner/repo[.git]`) and
/// SSH (`git@github.com:owner/repo[.git]`) formats, stripping credentials and
/// the `.git` suffix so the result is always `"owner/repo"`.
pub(crate) async fn read_git_remote_origin(project_path: &Path) -> Option<String> {
    let content = fs::read_to_string(project_path.join(".git/config"))
        .await
        .ok()?;
    let mut in_origin = false;
    for line in content.lines() {
        let t = line.trim();
        if t == r#"[remote "origin"]"# {
            in_origin = true;
        } else if t.starts_with('[') {
            in_origin = false;
        } else if in_origin
            && let Some(rest) = t.strip_prefix("url")
            && let Some(url) = rest.trim().strip_prefix('=')
        {
            return normalize_git_remote_to_owner_repo(url.trim());
        }
    }
    None
}

/// Extract `"owner/repo"` from a git remote URL, stripping credentials and `.git` suffix.
///
/// Supported formats:
/// - `https://github.com/owner/repo.git`
/// - `https://token@github.com/owner/repo`
/// - `git@github.com:owner/repo.git`
/// - `ssh://git@github.com/owner/repo`
pub(crate) fn normalize_git_remote_to_owner_repo(url: &str) -> Option<String> {
    let raw = if let Some(ssh_path) = url.strip_prefix("git@") {
        // git@github.com:owner/repo.git → take everything after the first `:`
        ssh_path.split_once(':')?.1.to_string()
    } else {
        // HTTPS: strip scheme, strip credentials (user:pass@host or token@host)
        let without_scheme = url.find("://").map(|i| &url[i + 3..]).unwrap_or(url);
        // Strip optional `user[:pass]@` prefix
        let without_creds = without_scheme
            .find('@')
            .map(|i| &without_scheme[i + 1..])
            .unwrap_or(without_scheme);
        // Strip hostname (everything up to the first `/`)
        without_creds
            .split_once('/')
            .map(|x| x.1)
            .map(str::to_string)?
    };
    // Strip trailing `.git`
    let slug = raw.strip_suffix(".git").unwrap_or(&raw);
    // Keep only `owner/repo` (first two path segments)
    let mut parts = slug.splitn(3, '/');
    let owner = parts.next().filter(|s| !s.is_empty())?;
    let repo = parts.next().filter(|s| !s.is_empty())?;
    Some(format!("{owner}/{repo}"))
}

#[cfg(test)]
#[path = "sessions_decode_tests.rs"]
mod decode_tests;
