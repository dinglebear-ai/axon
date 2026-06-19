// ── System / discovery results ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourcesResult {
    pub count: usize,
    pub limit: usize,
    pub offset: usize,
    /// Indexed URLs paired with their chunk counts.
    pub urls: Vec<(String, usize)>,
    /// Optional per-schema-version chunk counts (populated only when the
    /// caller opts in via `--by-schema-version`). Implicit pre-`axon_rust-lu6a`
    /// points (no `payload_schema_version` field) are reported under the
    /// key `1`. See `services::system::sources_with_breakdown`.
    pub schema_version_breakdown: Option<std::collections::BTreeMap<u32, usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DomainSourcesResult {
    pub domain: String,
    pub count: usize,
    pub limit: usize,
    pub cursor: Option<String>,
    pub next_cursor: Option<String>,
    pub truncated: bool,
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DomainFacet {
    pub domain: String,
    pub vectors: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DomainsResult {
    pub domains: Vec<DomainFacet>,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DomainIndexedResult {
    pub domain: String,
    pub indexed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DetailedDomainFacet {
    pub domain: String,
    pub vectors: usize,
    pub urls: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DetailedDomainsResult {
    pub domains: Vec<DetailedDomainFacet>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StatsResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CollectionsResult {
    pub collections: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DoctorResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DebugResult {
    pub payload: serde_json::Value,
}

/// True DB-level job counts across all job types.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StatusTotals {
    pub crawl: i64,
    pub extract: i64,
    pub embed: i64,
    pub ingest: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StatusResult {
    pub payload: serde_json::Value,
    pub text: String,
    pub totals: StatusTotals,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ServiceJob {
    pub id: uuid::Uuid,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error_text: Option<String>,
    pub url: Option<String>,
    pub source_type: Option<String>,
    pub target: Option<String>,
    pub urls_json: Option<serde_json::Value>,
    pub progress_json: Option<serde_json::Value>,
    pub result_json: Option<serde_json::Value>,
    pub config_json: Option<serde_json::Value>,
    pub attempt_count: i64,
    pub active_attempt_id: Option<String>,
    pub last_reclaimed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_reclaimed_reason: Option<String>,
}

// ── From<XJob> for ServiceJob ────────────────────────────────────────────────

impl From<crate::jobs::crawl::CrawlJob> for ServiceJob {
    fn from(job: crate::jobs::crawl::CrawlJob) -> Self {
        Self {
            id: job.id,
            status: job.status,
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error_text: job.error_text,
            url: Some(job.url),
            source_type: None,
            target: None,
            urls_json: None,
            progress_json: None,
            result_json: job.result_json,
            config_json: None,
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }
}

impl From<crate::jobs::embed::EmbedJob> for ServiceJob {
    fn from(job: crate::jobs::embed::EmbedJob) -> Self {
        Self {
            id: job.id,
            status: job.status,
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error_text: job.error_text,
            url: None,
            source_type: None,
            target: Some(job.input_text),
            urls_json: None,
            progress_json: None,
            result_json: job.result_json,
            config_json: Some(job.config_json),
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }
}

impl From<crate::jobs::extract::ExtractJob> for ServiceJob {
    fn from(job: crate::jobs::extract::ExtractJob) -> Self {
        Self {
            id: job.id,
            status: job.status,
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error_text: job.error_text,
            url: None,
            source_type: None,
            target: None,
            urls_json: Some(job.urls_json),
            progress_json: None,
            result_json: job.result_json,
            config_json: None,
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }
}

impl From<crate::jobs::ingest::IngestJob> for ServiceJob {
    fn from(job: crate::jobs::ingest::IngestJob) -> Self {
        Self {
            id: job.id,
            status: job.status,
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error_text: job.error_text,
            url: None,
            source_type: Some(job.source_type),
            target: Some(job.target),
            urls_json: None,
            progress_json: None,
            result_json: job.result_json,
            config_json: Some(job.config_json),
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }
}

// ── Named constructors ────────────────────────────────────────────────────────

impl ServiceJob {
    pub fn status_enum(&self) -> crate::jobs::status::JobStatus {
        crate::jobs::status::JobStatus::from_str(&self.status)
    }

    pub fn wire_json_compat(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}));
        let Some(obj) = value.as_object_mut() else {
            return value;
        };
        let active = self.status_enum().is_active();
        let metrics = if active {
            usable_progress_json(self.progress_json.as_ref()).or(self.result_json.as_ref())
        } else {
            self.result_json.as_ref()
        };
        if let Some(metrics) = metrics {
            obj.insert("metrics".to_string(), metrics.clone());
            if active {
                obj.insert("result_json".to_string(), metrics.clone());
            }
        }
        value
    }

    pub fn from_status_row(row: crate::jobs::backend::JobStatusRow) -> Self {
        Self {
            id: row.id,
            status: row.status.as_str().to_string(),
            created_at: row.created_at,
            updated_at: row.updated_at,
            started_at: row.started_at,
            finished_at: row.finished_at,
            error_text: row.error_text,
            url: None,
            source_type: None,
            target: None,
            urls_json: None,
            progress_json: row.progress_json,
            result_json: row.result_json,
            config_json: None,
            attempt_count: row.attempt_count,
            active_attempt_id: row.active_attempt_id,
            last_reclaimed_at: row.last_reclaimed_at,
            last_reclaimed_reason: row.last_reclaimed_reason,
        }
    }

    pub fn from_summary(summary: crate::jobs::backend::JobSummary) -> Self {
        Self {
            id: summary.id,
            status: summary.status.as_str().to_string(),
            created_at: summary.created_at,
            // JobSummary carries no updated_at; use created_at as a floor value.
            updated_at: summary.created_at,
            started_at: None,
            finished_at: None,
            error_text: None,
            url: None,
            source_type: None,
            target: Some(summary.target),
            urls_json: None,
            progress_json: None,
            result_json: None,
            config_json: None,
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }
}

fn usable_progress_json(value: Option<&serde_json::Value>) -> Option<&serde_json::Value> {
    value.filter(|value| {
        !(value.get("degraded").and_then(serde_json::Value::as_bool) == Some(true)
            && value.get("field").and_then(serde_json::Value::as_str) == Some("progress_json"))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DedupeResult {
    pub completed: bool,
    pub duplicate_groups: usize,
    pub deleted: usize,
}
