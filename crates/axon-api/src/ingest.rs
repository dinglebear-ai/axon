//! Transport-neutral ingest-source DTOs.
//!
//! `IngestSource` discriminates which external source an ingest job targets and
//! is shared by the ingest drivers, the job runners, the services layer, and the
//! CLI. It lives here (not in `axon-jobs`) so `axon-ingest` can classify targets
//! without depending on the jobs crate. The job *row* type (`IngestJob`, which
//! carries DB/`JobStatus` concerns) stays in `axon-jobs`.

use serde::{Deserialize, Serialize};

/// Discriminates which ingest source a job targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source_type", rename_all = "lowercase")]
pub enum IngestSource {
    Github {
        repo: String,
        include_source: bool,
    },
    Gitlab {
        target: String,
        include_source: bool,
    },
    Gitea {
        target: String,
        include_source: bool,
    },
    GenericGit {
        target: String,
        include_source: bool,
    },
    Reddit {
        target: String,
    },
    Youtube {
        target: String,
    },
    Rss {
        target: String,
    },
    Sessions {
        sessions_claude: bool,
        sessions_codex: bool,
        sessions_gemini: bool,
        sessions_project: Option<String>,
    },
    #[serde(rename = "prepared_sessions")]
    PreparedSessions {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestJobConfig {
    pub source: IngestSource,
    pub collection: String,
}

/// `source_type` values whose `(source_type, target)` pair fully identifies the
/// job's work — the stored target round-trips through `classify_target` into a
/// re-runnable job. Sessions are excluded: their content arrives via a sidecar
/// payload, not the target string. Single source of truth for `axon refresh`
/// origin classification — extend it when adding a provider.
pub const RE_INGESTABLE_SOURCE_TYPES: &[&str] = &[
    "github", "gitlab", "gitea", "git", "reddit", "youtube", "rss",
];

pub fn source_type_label(source: &IngestSource) -> &'static str {
    match source {
        IngestSource::Github { .. } => "github",
        IngestSource::Gitlab { .. } => "gitlab",
        IngestSource::Gitea { .. } => "gitea",
        IngestSource::GenericGit { .. } => "git",
        IngestSource::Reddit { .. } => "reddit",
        IngestSource::Youtube { .. } => "youtube",
        IngestSource::Rss { .. } => "rss",
        IngestSource::Sessions { .. } => "sessions",
        IngestSource::PreparedSessions { .. } => "prepared_sessions",
    }
}

pub fn target_label(source: &IngestSource) -> String {
    match source {
        IngestSource::Github { repo, .. } => repo.clone(),
        IngestSource::Gitlab { target, .. } => target.clone(),
        IngestSource::Gitea { target, .. } => target.clone(),
        IngestSource::GenericGit { target, .. } => target.clone(),
        IngestSource::Reddit { target } => target.clone(),
        IngestSource::Youtube { target } => target.clone(),
        IngestSource::Rss { target } => target.clone(),
        IngestSource::Sessions {
            sessions_claude,
            sessions_codex,
            sessions_gemini,
            sessions_project,
        } => {
            let all = !sessions_claude && !sessions_codex && !sessions_gemini;
            let label = if all {
                "all".to_string()
            } else {
                let mut parts = vec![];
                if *sessions_claude {
                    parts.push("claude");
                }
                if *sessions_codex {
                    parts.push("codex");
                }
                if *sessions_gemini {
                    parts.push("gemini");
                }
                parts.join(",")
            };
            match sessions_project {
                Some(proj) => format!("{label}:{proj}"),
                None => label,
            }
        }
        IngestSource::PreparedSessions { .. } => "prepared_sessions".to_string(),
    }
}
