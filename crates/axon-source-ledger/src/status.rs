use crate::SourceKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourcePhase {
    Idle,
    BackingOff,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceStatus {
    pub source_id: String,
    pub source_kind: SourceKind,
    pub phase: SourcePhase,
    pub committed_generation: i64,
    pub active_generation: Option<i64>,
    pub backoff_until_ms: Option<i64>,
    pub last_error: Option<String>,
    pub cleanup_debt_count: i64,
    pub updated_at_ms: i64,
}
