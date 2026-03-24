use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_done, log_info, log_warn};
use crate::crates::ingest::progress::PhaseReporter;
use crate::crates::jobs::common::make_pool;
use crate::crates::vector::ops::{PreparedDoc, embed_prepared_docs};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::fs;

const PHASE_SCANNING: &str = "scanning_sessions";
const PHASE_EMBEDDING: &str = "embedding_sessions";

mod claude;
mod codex;
mod gemini;

pub(crate) type IngestResult<T> = Result<T, anyhow::Error>;

/// A parsed session document ready for embedding, with state-tracking metadata.
pub(crate) struct SessionDoc {
    pub(crate) doc: PreparedDoc,
    pub(crate) collection: String,
    pub(crate) path: PathBuf,
    pub(crate) mtime: SystemTime,
    pub(crate) size: u64,
}

pub(crate) fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

pub(crate) struct SessionStateTracker {
    pool: Option<PgPool>,
}

impl SessionStateTracker {
    pub(crate) async fn new(cfg: &Config) -> Self {
        match make_pool(cfg).await {
            Ok(pool) => {
                if let Err(e) = sqlx::query(
                    r#"
                    CREATE TABLE IF NOT EXISTS axon_session_ingest_state (
                        file_path TEXT PRIMARY KEY,
                        last_modified TIMESTAMPTZ NOT NULL,
                        file_size BIGINT NOT NULL,
                        indexed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
                    )
                    "#,
                )
                .execute(&pool)
                .await
                {
                    log_warn(&format!("failed to ensure session state table: {e}"));
                    return Self { pool: None };
                }
                Self { pool: Some(pool) }
            }
            Err(e) => {
                log_warn(&format!(
                    "database connection failed, state tracking disabled: {e}"
                ));
                Self { pool: None }
            }
        }
    }

    pub(crate) async fn should_skip(&self, path: &Path, mtime: SystemTime, size: u64) -> bool {
        let Some(pool) = &self.pool else {
            return false;
        };
        let path_str = path.to_string_lossy().to_string();

        let row = sqlx::query(
            "SELECT last_modified, file_size FROM axon_session_ingest_state WHERE file_path = $1",
        )
        .bind(path_str)
        .fetch_optional(pool)
        .await;

        match row {
            Ok(Some(r)) => {
                let db_mtime: chrono::DateTime<chrono::Utc> = r.get(0);
                let db_size: i64 = r.get(1);
                let current_mtime: chrono::DateTime<chrono::Utc> = mtime.into();
                (db_mtime - current_mtime).num_seconds().abs() < 1 && db_size == (size as i64)
            }
            _ => false,
        }
    }

    pub(crate) async fn mark_indexed(&self, path: &Path, mtime: SystemTime, size: u64) {
        let Some(pool) = &self.pool else {
            return;
        };
        let path_str = path.to_string_lossy().to_string();
        let mtime_chrono: chrono::DateTime<chrono::Utc> = mtime.into();

        let _ = sqlx::query(
            r#"
            INSERT INTO axon_session_ingest_state (file_path, last_modified, file_size, indexed_at)
            VALUES ($1, $2, $3, NOW())
            ON CONFLICT (file_path) DO UPDATE
            SET last_modified = EXCLUDED.last_modified,
                file_size = EXCLUDED.file_size,
                indexed_at = NOW()
            "#,
        )
        .bind(path_str)
        .bind(mtime_chrono)
        .bind(size as i64)
        .execute(pool)
        .await;
    }
}

pub async fn ingest_sessions(
    cfg: &Config,
    reporter: &PhaseReporter,
) -> Result<usize, Box<dyn Error>> {
    log_info("command=ingest source=sessions");
    reporter.report_phase(PHASE_SCANNING).await;

    let state = SessionStateTracker::new(cfg).await;
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
        let docs = claude::collect_claude_docs(cfg, &state, &multi)
            .await
            .unwrap_or_default();
        log_info(&format!("sessions platform=claude files={}", docs.len()));
        all_docs.extend(docs);
    }
    if cfg.sessions_codex || all_platforms {
        let docs = codex::collect_codex_docs(cfg, &state, &multi)
            .await
            .unwrap_or_default();
        log_info(&format!("sessions platform=codex files={}", docs.len()));
        all_docs.extend(docs);
    }
    if cfg.sessions_gemini || all_platforms {
        let docs = gemini::collect_gemini_docs(cfg, &state, &multi)
            .await
            .unwrap_or_default();
        log_info(&format!("sessions platform=gemini files={}", docs.len()));
        all_docs.extend(docs);
    }

    reporter.report_phase(PHASE_EMBEDDING).await;
    main_pb.set_message(format!("Embedding {} session files...", all_docs.len()));

    let total_chunks = embed_all_session_docs(cfg, all_docs, &state).await;

    main_pb.finish_with_message(format!("Done: {} chunks embedded", total_chunks));
    log_done(&format!(
        "command=ingest source=sessions total_chunk_count={total_chunks}"
    ));
    Ok(total_chunks)
}

/// Groups collected docs by collection, calls `embed_prepared_docs` once per
/// collection, then marks all successfully embedded files as indexed.
async fn embed_all_session_docs(
    cfg: &Config,
    docs: Vec<SessionDoc>,
    state: &SessionStateTracker,
) -> usize {
    let mut by_collection: HashMap<String, Vec<SessionDoc>> = HashMap::new();
    for doc in docs {
        by_collection
            .entry(doc.collection.clone())
            .or_default()
            .push(doc);
    }

    let mut total = 0;
    for (collection, session_docs) in by_collection {
        let mut session_cfg = cfg.clone();
        session_cfg.collection = collection;

        let (state_meta, prepared): (Vec<_>, Vec<_>) = session_docs
            .into_iter()
            .map(|sd| ((sd.path, sd.mtime, sd.size), sd.doc))
            .unzip();
        // Consume the Result fully before any subsequent .await so Box<dyn Error>
        // (which is !Send) does not live across an await point.
        let embed_ok = match embed_prepared_docs(&session_cfg, prepared, None).await {
            Ok(summary) => {
                total += summary.chunks_embedded;
                true
            }
            Err(e) => {
                log_warn(&format!(
                    "sessions embed failed collection={} error={e}",
                    session_cfg.collection
                ));
                false
            }
        };
        if embed_ok {
            for (path, mtime, size) in &state_meta {
                state.mark_indexed(path, *mtime, *size).await;
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
    if cfg.collection != "cortex" {
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
            return Some(url.trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod decode_tests {
    use super::{decode_claude_project_path, decode_path_walk};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn mk(tmp: &TempDir, rel: &str) -> PathBuf {
        let p = tmp.path().join(rel);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn decode_empty_returns_none() {
        assert!(decode_claude_project_path("").is_none());
        assert!(decode_claude_project_path("-").is_none());
    }

    #[test]
    fn decode_simple_path_with_dashes() {
        let tmp = TempDir::new().unwrap();
        mk(&tmp, "home/user/workspace/unraid-api");
        // Construct dir name as Claude would: replace `/` with `-`
        // `/tmp/xxx/home/user/workspace/unraid-api` — we test decode_path_walk directly
        // to avoid hardcoding the system `/` root.
        let parts: Vec<String> = vec!["home", "user", "workspace", "unraid", "api"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let result = decode_path_walk(tmp.path(), &parts, 0);
        assert_eq!(
            result,
            Some(tmp.path().join("home/user/workspace/unraid-api"))
        );
    }

    #[test]
    fn decode_prefers_dash_dir_over_underscore_when_both_exist() {
        // If a real `axon-rust` dir exists it should be found before `axon_rust` variant
        let tmp = TempDir::new().unwrap();
        mk(&tmp, "home/user/axon-rust");
        let parts: Vec<String> = vec!["home", "user", "axon", "rust"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let result = decode_path_walk(tmp.path(), &parts, 0);
        assert_eq!(result, Some(tmp.path().join("home/user/axon-rust")));
    }

    #[test]
    fn decode_falls_back_to_underscore_variant() {
        // No `axon-rust`, but `axon_rust` exists — should find it
        let tmp = TempDir::new().unwrap();
        mk(&tmp, "home/user/axon_rust");
        let parts: Vec<String> = vec!["home", "user", "axon", "rust"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let result = decode_path_walk(tmp.path(), &parts, 0);
        assert_eq!(result, Some(tmp.path().join("home/user/axon_rust")));
    }

    #[test]
    fn decode_literal_dash_via_double_dash_encoding() {
        // dir name `-home-user--my-project` → path `/home/user/-my-project`
        // After stripping leading `-` and replacing `--` with placeholder:
        // parts = ["home", "user", "-my-project"] (the `-` is restored from `--`)
        // But `decode_claude_project_path` splits on single `-` after placeholder substitution.
        let tmp = TempDir::new().unwrap();
        mk(&tmp, "home/user/-my-project");
        let parts: Vec<String> = vec!["home", "user", "-my-project"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let result = decode_path_walk(tmp.path(), &parts, 0);
        assert_eq!(result, Some(tmp.path().join("home/user/-my-project")));
    }

    #[test]
    fn decode_returns_none_when_no_matching_dir() {
        let tmp = TempDir::new().unwrap();
        let parts: Vec<String> = vec!["home", "nobody", "nonexistent"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert!(decode_path_walk(tmp.path(), &parts, 0).is_none());
    }
}
