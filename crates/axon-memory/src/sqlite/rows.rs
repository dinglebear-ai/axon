//! Row <-> DTO conversion for `memory_records`.
//!
//! JSON columns (`decay_json`, `history_json`, `embedding_refs_json`) are
//! serialized with `serde_json`; enum columns store the snake_case wire form.

use axon_api::source::{
    MemoryDecayPolicy, MemoryHistoryEvent, MemoryId, MemoryLink, MemoryRecord, MemoryScope,
    MemoryStatus, MemoryType, VectorPointId, Visibility,
};
use rusqlite::Row;

use crate::sqlite::error::store_error;
use crate::store::Result;

/// Serialize a memory type to its snake_case wire string.
pub fn type_to_str(t: MemoryType) -> &'static str {
    match t {
        MemoryType::Decision => "decision",
        MemoryType::Fact => "fact",
        MemoryType::Preference => "preference",
        MemoryType::Task => "task",
        MemoryType::Bug => "bug",
        MemoryType::Procedure => "procedure",
        MemoryType::Incident => "incident",
        MemoryType::Entity => "entity",
        MemoryType::Episode => "episode",
        MemoryType::Working => "working",
    }
}

/// Parse a memory type from its wire string.
pub fn type_from_str(s: &str) -> Result<MemoryType> {
    Ok(match s {
        "decision" => MemoryType::Decision,
        "fact" => MemoryType::Fact,
        "preference" => MemoryType::Preference,
        "task" => MemoryType::Task,
        "bug" => MemoryType::Bug,
        "procedure" => MemoryType::Procedure,
        "incident" => MemoryType::Incident,
        "entity" => MemoryType::Entity,
        "episode" => MemoryType::Episode,
        "working" => MemoryType::Working,
        other => return Err(store_error(format!("unknown memory_type {other}"))),
    })
}

/// Serialize a memory status to its snake_case wire string.
pub fn status_to_str(s: MemoryStatus) -> &'static str {
    match s {
        MemoryStatus::Active => "active",
        MemoryStatus::Review => "review",
        MemoryStatus::Superseded => "superseded",
        MemoryStatus::Contradicted => "contradicted",
        MemoryStatus::Archived => "archived",
        MemoryStatus::Forgotten => "forgotten",
        MemoryStatus::Working => "working",
    }
}

/// Parse a memory status from its wire string.
pub fn status_from_str(s: &str) -> Result<MemoryStatus> {
    Ok(match s {
        "active" => MemoryStatus::Active,
        "review" => MemoryStatus::Review,
        "superseded" => MemoryStatus::Superseded,
        "contradicted" => MemoryStatus::Contradicted,
        "archived" => MemoryStatus::Archived,
        "forgotten" => MemoryStatus::Forgotten,
        "working" => MemoryStatus::Working,
        other => return Err(store_error(format!("unknown memory_status {other}"))),
    })
}

/// Serialize a visibility classification to its snake_case wire string.
pub fn visibility_to_str(v: Visibility) -> &'static str {
    match v {
        Visibility::Public => "public",
        Visibility::Internal => "internal",
        Visibility::Sensitive => "sensitive",
        Visibility::Redacted => "redacted",
        Visibility::Derived => "derived",
    }
}

/// Parse a visibility classification from its wire string.
pub fn visibility_from_str(s: &str) -> Result<Visibility> {
    Ok(match s {
        "public" => Visibility::Public,
        "internal" => Visibility::Internal,
        "sensitive" => Visibility::Sensitive,
        "redacted" => Visibility::Redacted,
        "derived" => Visibility::Derived,
        other => return Err(store_error(format!("unknown visibility {other}"))),
    })
}

/// Build a `MemoryRecord` from a `memory_records` row joined with its links.
pub fn record_from_row(row: &Row, links: Vec<MemoryLink>) -> Result<MemoryRecord> {
    let memory_id: String = row.get("memory_id").map_err(map_sql)?;
    let type_str: String = row.get("memory_type").map_err(map_sql)?;
    let status_str: String = row.get("status").map_err(map_sql)?;
    let body: String = row.get("body").map_err(map_sql)?;
    let title: Option<String> = row.get("title").map_err(map_sql)?;
    let visibility_str: String = row.get("visibility").map_err(map_sql)?;
    let confidence: f64 = row.get("confidence").map_err(map_sql)?;
    let salience: f64 = row.get("salience").map_err(map_sql)?;
    let scope_kind: String = row.get("scope_kind").map_err(map_sql)?;
    let scope_value: String = row.get("scope_value").map_err(map_sql)?;
    let decay_json: Option<String> = row.get("decay_json").map_err(map_sql)?;
    let history_json: String = row.get("history_json").map_err(map_sql)?;
    let embedding_json: String = row.get("embedding_refs_json").map_err(map_sql)?;
    let superseded_by: Option<String> = row.get("superseded_by").map_err(map_sql)?;
    let contradicts: Option<String> = row.get("contradicts").map_err(map_sql)?;

    let decay: Option<MemoryDecayPolicy> = match decay_json {
        Some(json) if !json.is_empty() => Some(serde_json::from_str(&json).map_err(map_json)?),
        _ => None,
    };
    let history: Vec<MemoryHistoryEvent> = serde_json::from_str(&history_json).map_err(map_json)?;
    let embedding_refs: Vec<VectorPointId> =
        serde_json::from_str(&embedding_json).map_err(map_json)?;

    Ok(MemoryRecord {
        memory_id: MemoryId::new(memory_id),
        memory_type: type_from_str(&type_str)?,
        status: status_from_str(&status_str)?,
        body,
        confidence: confidence as f32,
        salience: salience as f32,
        scope: MemoryScope {
            kind: scope_kind,
            value: scope_value,
        },
        history,
        visibility: visibility_from_str(&visibility_str)?,
        title,
        links,
        decay,
        embedding_refs,
        superseded_by: superseded_by.map(MemoryId::new),
        contradicts: contradicts.map(MemoryId::new),
    })
}

/// Serialize `decay`/`history`/`embedding_refs` to JSON column strings.
pub fn record_json_columns(record: &MemoryRecord) -> Result<(Option<String>, String, String)> {
    let decay_json = match &record.decay {
        Some(decay) => Some(serde_json::to_string(decay).map_err(map_json)?),
        None => None,
    };
    let history_json = serde_json::to_string(&record.history).map_err(map_json)?;
    let embedding_json = serde_json::to_string(&record.embedding_refs).map_err(map_json)?;
    Ok((decay_json, history_json, embedding_json))
}

fn map_sql(err: rusqlite::Error) -> axon_api::source::ApiError {
    store_error(format!("sqlite: {err}"))
}

fn map_json(err: serde_json::Error) -> axon_api::source::ApiError {
    store_error(format!("json: {err}"))
}
