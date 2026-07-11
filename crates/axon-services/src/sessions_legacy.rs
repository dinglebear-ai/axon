use crate::ingest::progress::PhaseReporter;
use crate::sessions_legacy::watch::validate::{SessionProvider, ValidatedSessionPath};
use axon_core::config::Config;
use axon_core::logging::{log_done, log_info, log_warn};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;

const PHASE_SCANNING: &str = "scanning_sessions";
const PHASE_EMBEDDING: &str = "embedding_sessions";
const SESSION_INGEST_MAX_BYTES_ENV: &str = "AXON_SESSION_INGEST_MAX_BYTES";
const DEFAULT_SESSION_INGEST_MAX_BYTES: u64 = 20 * 1024 * 1024;

pub mod checkpoint;
mod claude;
mod codex;
mod embed_batch;
mod gemini;
mod prepared;
mod session_doc;
pub mod watch;

use embed_batch::{embed_all_session_docs, embed_session_docs};
#[cfg(test)]
pub use prepared::MAX_PREPARED_SESSION_DOCS;
pub use prepared::{IngestSessionsPreparedRequest, PreparedSessionDoc};
pub(crate) use session_doc::SessionDoc;

pub(crate) type IngestResult<T> = Result<T, anyhow::Error>;

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

pub(crate) fn session_ingest_max_bytes_for_config(_cfg: &Config) -> usize {
    usize::try_from(session_ingest_max_bytes()).unwrap_or(usize::MAX)
}

pub fn redact_session_text(input: &str) -> String {
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

async fn collect_prepared_session_docs(
    cfg: &Config,
) -> Result<(Vec<PreparedSessionDoc>, Option<String>), Box<dyn Error>> {
    let multi = MultiProgress::new();
    let all_platforms = !cfg.sessions_claude && !cfg.sessions_codex && !cfg.sessions_gemini;
    let mut all_docs: Vec<SessionDoc> = Vec::new();

    if cfg.sessions_claude || all_platforms {
        all_docs.extend(claude::collect_claude_docs(cfg, &multi).await?);
    }
    if cfg.sessions_codex || all_platforms {
        all_docs.extend(codex::collect_codex_docs(cfg, &multi).await?);
    }
    if cfg.sessions_gemini || all_platforms {
        all_docs.extend(gemini::collect_gemini_docs(cfg, &multi).await?);
    }

    let collection = if cfg.collection != "axon" {
        Some(cfg.collection.clone())
    } else {
        None
    };
    let docs = all_docs
        .into_iter()
        .map(prepared_session_doc_from_session_doc)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| -> Box<dyn Error> { err.into() })?;
    Ok((docs, collection))
}

pub async fn prepare_sessions_request(
    cfg: &Config,
) -> Result<IngestSessionsPreparedRequest, Box<dyn Error>> {
    let (docs, collection) = collect_prepared_session_docs(cfg).await?;
    let request = IngestSessionsPreparedRequest {
        docs,
        project: cfg.sessions_project.clone(),
        collection,
    };
    Ok(request)
}

pub async fn collect_prepared_session_file_doc(
    cfg: &Config,
    path: &Path,
) -> Result<Option<PreparedSessionDoc>, Box<dyn Error>> {
    let roots = watch::validate::SessionWatchRoots::from_config(cfg)?;
    let validated = watch::validate::validate_session_file_path(&roots, path)?;
    let doc = collect_session_file_doc(cfg, &validated)
        .await?
        .map(prepared_session_doc_from_session_doc)
        .transpose()
        .map_err(|err| -> Box<dyn Error> { err.into() })?;
    Ok(doc)
}

pub(crate) async fn collect_session_file_doc(
    cfg: &Config,
    validated: &ValidatedSessionPath,
) -> Result<Option<SessionDoc>, Box<dyn Error>> {
    match validated.provider {
        SessionProvider::Claude => {
            claude::collect_claude_file_doc(cfg, validated.canonical.clone())
                .await
                .map_err(|err| -> Box<dyn Error> { err.into() })
        }
        SessionProvider::Codex => codex::collect_codex_file_doc(cfg, validated.canonical.clone())
            .await
            .map_err(|err| -> Box<dyn Error> { err.into() }),
        SessionProvider::Gemini => {
            gemini::collect_gemini_file_doc(cfg, validated.canonical.clone())
                .await
                .map_err(|err| -> Box<dyn Error> { err.into() })
        }
    }
}

pub(crate) fn has_supported_session_extension(provider: SessionProvider, path: &Path) -> bool {
    match provider {
        SessionProvider::Claude | SessionProvider::Codex => {
            path.extension().is_some_and(|ext| ext == "jsonl")
        }
        SessionProvider::Gemini => path.extension().is_some_and(|ext| ext == "json"),
    }
}

/// Scan local session exports and split them into validated batches, each within
/// the per-request limits of `/v1/ingest/sessions/prepared`, so large histories
/// can be uploaded as several jobs instead of failing the doc-count cap. Returns
/// an empty vec when no session docs match.
pub async fn prepare_sessions_request_batches(
    cfg: &Config,
) -> Result<Vec<IngestSessionsPreparedRequest>, Box<dyn Error>> {
    let (docs, collection) = collect_prepared_session_docs(cfg).await?;
    if docs.is_empty() {
        return Ok(Vec::new());
    }
    let project = cfg.sessions_project.clone();
    prepared::split_prepared_session_docs(docs, cfg)
        .into_iter()
        .map(|batch| {
            let request = IngestSessionsPreparedRequest {
                docs: batch,
                project: project.clone(),
                collection: collection.clone(),
            };
            request
                .validate(cfg)
                .map(|()| request)
                .map_err(|err| -> Box<dyn Error> { err.into() })
        })
        .collect()
}

pub(crate) fn prepared_session_doc_from_session_doc(
    session_doc: SessionDoc,
) -> Result<PreparedSessionDoc, String> {
    let SessionDoc {
        url,
        title,
        source_type,
        extra,
        raw_text: text,
        ..
    } = session_doc;
    let platform = match source_type {
        "claude_session" => "claude",
        "codex_session" => "codex",
        "gemini_session" => "gemini",
        other => return Err(format!("unsupported session source_type: {other}")),
    }
    .to_string();
    let extra = extra.unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
    let session_project = extra
        .get("project_name")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let session_date = extra
        .get("session_date")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let session_turn_count = extra
        .get("turn_count")
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok());
    let session_file = extra
        .get("session_file")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| url.strip_prefix("file://").unwrap_or(&url).to_string());

    Ok(PreparedSessionDoc {
        url,
        title,
        text,
        session_platform: platform,
        session_project,
        session_date,
        session_turn_count,
        session_file,
        extra,
    })
}


pub async fn ingest_prepared_sessions(
    cfg: &Config,
    request: IngestSessionsPreparedRequest,
    reporter: &PhaseReporter,
) -> Result<usize, Box<dyn Error>> {
    log_info("command=ingest source=prepared_sessions");
    reporter.report_phase(PHASE_EMBEDDING).await;
    let docs = request
        .into_session_docs(cfg)
        .map_err(|err| -> Box<dyn Error> { err.into() })?;
    let total_chunks = embed_session_docs(cfg, docs, true).await?;
    log_done(&format!(
        "command=ingest source=prepared_sessions total_chunk_count={total_chunks}"
    ));
    Ok(total_chunks)
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
#[path = "sessions_legacy_tests.rs"]
mod tests;

/// Session-level metadata collected once per project directory, injected into
/// every `SessionDoc.extra` produced by that project's session files.
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
#[path = "sessions_legacy_decode_tests.rs"]
mod decode_tests;
