use crate::SourceKind;
use serde::{Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourcePhase {
    Idle,
    BackingOff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceStatus {
    pub source_id: String,
    pub source_kind: SourceKind,
    pub phase: SourcePhase,
    pub committed_generation: i64,
    pub active_generation: Option<i64>,
    pub backoff_until_ms: Option<i64>,
    #[serde(serialize_with = "serialize_redacted_error")]
    pub last_error: Option<String>,
    pub cleanup_debt_count: i64,
    pub updated_at_ms: i64,
}

fn serialize_redacted_error<S>(value: &Option<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    value
        .as_ref()
        .map(|text| redact_error(text))
        .serialize(serializer)
}

fn redact_error(text: &str) -> String {
    if text.contains("Authorization")
        || text.contains("Cookie")
        || text.contains("/home/")
        || text.contains("\\Users\\")
    {
        "[redacted]".to_string()
    } else {
        text.to_string()
    }
}
