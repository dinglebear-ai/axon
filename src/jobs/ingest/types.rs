use crate::jobs::status::JobStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

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

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct IngestJob {
    pub id: Uuid,
    /// Raw status string from the database. Use [`IngestJob::status()`] for
    /// type-safe access when `JobStatus` gains `sqlx::Type` derive.
    pub status: String,
    pub source_type: String,
    pub target: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_text: Option<String>,
    pub result_json: Option<serde_json::Value>,
    pub config_json: serde_json::Value,
}

impl IngestJob {
    /// Parse the raw `status` string into a typed [`JobStatus`].
    ///
    /// Returns `None` if the string doesn't match any known variant (shouldn't
    /// happen with the CHECK constraint, but defensive is correct).
    pub fn status(&self) -> Option<JobStatus> {
        match self.status.as_str() {
            "pending" => Some(JobStatus::Pending),
            "running" => Some(JobStatus::Running),
            "completed" => Some(JobStatus::Completed),
            "failed" => Some(JobStatus::Failed),
            "canceled" => Some(JobStatus::Canceled),
            _ => None,
        }
    }
}

/// `source_type` values (see [`source_type_label`]) whose `(source_type,
/// target)` pair fully identifies the job's work — the stored target / payload
/// `seed_url` round-trips through `services::ingest::classify_target` into a
/// re-runnable job. Sessions are excluded: their content arrives via a sidecar
/// payload, not the target string. This is the single source of truth for
/// `axon refresh` origin classification — extend it when adding a provider.
pub const RE_INGESTABLE_SOURCE_TYPES: &[&str] = &[
    "github", "gitlab", "gitea", "git", "reddit", "youtube", "rss",
];

pub(crate) fn source_type_label(source: &IngestSource) -> &'static str {
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

pub(crate) fn target_label(source: &IngestSource) -> String {
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
