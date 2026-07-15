//! Transport DTOs and pure mapping helpers for the `memory` command path.
//!
//! These shapes and conversions are the CLI/MCP/REST-facing surface. They are
//! deliberately transport-only: no store access, no I/O — just validation,
//! redaction, scope/facet derivation, and `MemoryRecord` ⇄ `MemoryItem`
//! translation. The service functions in the parent module compose these with
//! the real [`axon_memory`] store.

use anyhow::{Result, bail};
use axon_adapters::sessions::redact_session_text;
use serde::{Deserialize, Serialize};

use super::runtime_metadata::detect_runtime_memory_metadata;
use axon_api::mcp_schema::{MemoryEdgeType, MemoryNodeType, MemoryRequest};
use axon_api::source::{MemoryLink, MemoryRecord, MemoryScope, MemoryStatus, MemoryType};

/// Link types used to persist the CLI project/repo/file scope facets.
pub(super) const LINK_PROJECT: &str = "memory_project";
pub(super) const LINK_REPO: &str = "memory_repo";
pub(super) const LINK_FILE: &str = "memory_file";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryItem {
    pub id: String,
    pub memory_type: String,
    pub title: String,
    pub body: Option<String>,
    pub project: Option<String>,
    pub repo: Option<String>,
    pub file: Option<String>,
    pub workspace: Option<String>,
    pub git_branch: Option<String>,
    pub git_commit: Option<String>,
    pub git_dirty: Option<bool>,
    pub cwd: Option<String>,
    pub confidence: f64,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_seen_at: i64,
    pub access_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryEdgeItem {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub edge_type: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryContext {
    pub context: String,
    pub memories: Vec<MemoryItem>,
    pub token_budget: usize,
    pub token_estimate: usize,
    pub truncated: bool,
}

/// Normalized `remember` inputs after validation/redaction/runtime autofill.
#[derive(Debug, Clone)]
pub(super) struct NormalizedMemory {
    pub(super) memory_type: String,
    pub(super) title: String,
    pub(super) body: String,
    pub(super) project: Option<String>,
    pub(super) repo: Option<String>,
    pub(super) file: Option<String>,
    pub(super) confidence: f64,
}

pub(super) fn normalize_remember(req: MemoryRequest) -> Result<NormalizedMemory> {
    if req.id.is_some() {
        bail!("id is not accepted for memory.remember; ids are server-generated");
    }
    let body = redact_session_text(required_text(req.body.as_deref(), "body")?);
    let title = req
        .title
        .as_deref()
        .map(redact_session_text)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| derive_title(&body));
    let memory_type = node_type_name(req.memory_type.unwrap_or(MemoryNodeType::Fact)).to_string();
    let confidence = req.confidence.unwrap_or(1.0);
    if !(0.0..=1.0).contains(&confidence) {
        bail!("confidence must be between 0.0 and 1.0");
    }
    let runtime = detect_runtime_memory_metadata();
    let project = clean_opt(req.project).or(runtime.project);
    let repo = clean_opt(req.repo).or(runtime.repo);
    let file = clean_opt(req.file);
    Ok(NormalizedMemory {
        memory_type,
        title,
        body,
        project,
        repo,
        file,
        confidence,
    })
}

/// Pick the narrowest scope for a memory: file > repo > project > global.
pub(super) fn scope_for(memory: &NormalizedMemory) -> MemoryScope {
    if let Some(file) = memory.file.as_deref() {
        MemoryScope {
            kind: "file".to_string(),
            value: file.to_string(),
        }
    } else if let Some(repo) = memory.repo.as_deref() {
        MemoryScope {
            kind: "repo".to_string(),
            value: repo.to_string(),
        }
    } else if let Some(project) = memory.project.as_deref() {
        MemoryScope {
            kind: "project".to_string(),
            value: project.to_string(),
        }
    } else {
        MemoryScope {
            kind: "global".to_string(),
            value: "global".to_string(),
        }
    }
}

/// Persist project/repo/file as evidence-free links so they round-trip on read.
pub(super) fn facet_links(memory: &NormalizedMemory) -> Vec<MemoryLink> {
    let mut links = Vec::new();
    for (link_type, value) in [
        (LINK_PROJECT, memory.project.as_deref()),
        (LINK_REPO, memory.repo.as_deref()),
        (LINK_FILE, memory.file.as_deref()),
    ] {
        if let Some(value) = value {
            links.push(MemoryLink {
                link_type: link_type.to_string(),
                target: value.to_string(),
                confidence: 1.0,
                evidence: Vec::new(),
            });
        }
    }
    links
}

/// Build a transport `MemoryItem` from a stored `MemoryRecord`.
pub(super) fn item_from_record(record: &MemoryRecord, score: Option<f64>) -> MemoryItem {
    let created = timestamp_ms(record.history.first().map(|e| e.timestamp.0.as_str()));
    let updated = timestamp_ms(record.history.last().map(|e| e.timestamp.0.as_str()));
    let access_count = record
        .decay
        .as_ref()
        .map(|d| d.reinforcement_count as i64)
        .unwrap_or(0);
    MemoryItem {
        id: record.memory_id.0.clone(),
        memory_type: type_name(record.memory_type).to_string(),
        title: record.title.clone().unwrap_or_default(),
        body: Some(record.body.clone()),
        project: facet_value(record, LINK_PROJECT),
        repo: facet_value(record, LINK_REPO),
        file: facet_value(record, LINK_FILE),
        workspace: None,
        git_branch: None,
        git_commit: None,
        git_dirty: None,
        cwd: None,
        confidence: record.confidence as f64,
        status: status_name(record.status).to_string(),
        created_at: created,
        updated_at: updated,
        last_seen_at: updated,
        access_count,
        score,
    }
}

/// Extract a persisted facet (project/repo/file) from a record's links.
fn facet_value(record: &MemoryRecord, link_type: &str) -> Option<String> {
    record
        .links
        .iter()
        .find(|l| l.link_type == link_type)
        .map(|l| l.target.clone())
}

pub(super) fn facet_matches(item: &MemoryItem, want: Option<&str>, facet: &str) -> bool {
    let Some(want) = clean_opt(want.map(str::to_string)) else {
        return true;
    };
    let have = match facet {
        "project" => item.project.as_deref(),
        "repo" => item.repo.as_deref(),
        "file" => item.file.as_deref(),
        _ => None,
    };
    have == Some(want.as_str())
}

pub(super) fn status_matches(item: &MemoryItem, want: Option<&str>) -> bool {
    match clean_opt(want.map(str::to_string)) {
        Some(status) => item.status == status,
        None => item.status == "active",
    }
}

/// Parse an RFC3339 timestamp into epoch milliseconds (0 when missing/bad).
fn timestamp_ms(ts: Option<&str>) -> i64 {
    ts.and_then(axon_memory::record::parse_epoch_secs)
        .map(|s| s * 1_000)
        .unwrap_or(0)
}

pub(super) fn required_text<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("{field} is required"))
}

pub(super) fn clean_opt(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn derive_title(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Untitled memory")
        .chars()
        .take(120)
        .collect()
}

/// Map the closed CLI `MemoryNodeType` (5 kinds) to the source `MemoryType`.
pub(super) fn parse_memory_type(value: &str) -> MemoryType {
    match value {
        "decision" => MemoryType::Decision,
        "preference" => MemoryType::Preference,
        "task" => MemoryType::Task,
        "bug" => MemoryType::Bug,
        _ => MemoryType::Fact,
    }
}

pub(super) fn node_type_name(value: MemoryNodeType) -> &'static str {
    match value {
        MemoryNodeType::Decision => "decision",
        MemoryNodeType::Fact => "fact",
        MemoryNodeType::Preference => "preference",
        MemoryNodeType::Task => "task",
        MemoryNodeType::Bug => "bug",
    }
}

/// Render the source `MemoryType` to the CLI wire string. The extra source-only
/// kinds collapse to `fact` for the CLI surface.
fn type_name(value: MemoryType) -> &'static str {
    match value {
        MemoryType::Decision => "decision",
        MemoryType::Preference => "preference",
        MemoryType::Task => "task",
        MemoryType::Bug => "bug",
        MemoryType::Fact
        | MemoryType::Procedure
        | MemoryType::Incident
        | MemoryType::Entity
        | MemoryType::Episode
        | MemoryType::Working => "fact",
    }
}

fn status_name(value: MemoryStatus) -> &'static str {
    match value {
        MemoryStatus::Active => "active",
        MemoryStatus::Review => "review",
        MemoryStatus::Superseded => "superseded",
        MemoryStatus::Contradicted => "contradicted",
        MemoryStatus::Archived => "archived",
        MemoryStatus::Forgotten => "forgotten",
        MemoryStatus::Working => "working",
    }
}

pub(super) fn edge_type_name(value: MemoryEdgeType) -> &'static str {
    match value {
        MemoryEdgeType::RelatesTo => "relates_to",
        MemoryEdgeType::Supersedes => "supersedes",
    }
}
