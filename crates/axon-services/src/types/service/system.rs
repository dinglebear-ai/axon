// ── System / discovery results ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourcesResult {
    pub count: usize,
    pub limit: usize,
    pub offset: usize,
    /// Indexed URLs paired with their chunk counts.
    pub urls: Vec<(String, usize)>,
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
    pub source: i64,
    pub extract: i64,
    pub watch: i64,
    pub prune: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StatusResult {
    pub payload: serde_json::Value,
    pub text: String,
    pub totals: StatusTotals,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// `ServiceJob` now lives in `axon_api::service_job`; the `From<*Job>` conversions
/// live in `axon_jobs::service_job_conv`. Re-exported so existing
/// `crate::types::ServiceJob` call sites resolve unchanged.
pub use axon_api::service_job::ServiceJob;
