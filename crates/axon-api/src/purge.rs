//! Transport-neutral DTO for the `purge` operation (delete indexed points by
//! URL / seed-URL prefix).
//!
//! Lives in `axon-api` — not `axon-services` — per the workspace architecture
//! rule: a contract type is owned by the layer that the domain logic returns,
//! and consumed directly by every transport. `axon-vector` (which owns the
//! delete logic) returns this; CLI/MCP/REST/palette all format it.

/// Result of a purge: counts of points/URLs matched (and deleted, unless this
/// was a `dry_run` preview).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PurgeResult {
    pub target: String,
    pub prefix: bool,
    /// When true, nothing was deleted — counts reflect what *would* be removed.
    pub dry_run: bool,
    pub matched_points: usize,
    pub deleted_points: usize,
    pub matched_url_count: usize,
    pub sample_urls: Vec<String>,
}
