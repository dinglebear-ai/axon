use axon_core::config::Config;
use axon_core::content::url_to_domain;
use axon_vector::ops::{PreparedDoc, prepare_plain_text_source};
use serde::{Deserialize, Serialize};

pub const MAX_PREPARED_SESSION_DOCS: usize = 256;
const MAX_PREPARED_SESSION_METADATA_BYTES: usize = 64 * 1024;
const RESERVED_EXTRA_KEYS: &[&str] = &[
    "agent",
    "project_name",
    "session_date",
    "turn_count",
    "session_file",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedSessionDoc {
    pub url: String,
    pub title: Option<String>,
    pub text: String,
    pub session_platform: String,
    pub session_project: Option<String>,
    pub session_date: Option<String>,
    pub session_turn_count: Option<u32>,
    pub session_file: String,
    #[serde(default)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct IngestSessionsPreparedRequest {
    pub docs: Vec<PreparedSessionDoc>,
    pub project: Option<String>,
    pub collection: Option<String>,
}

impl IngestSessionsPreparedRequest {
    pub fn validate(&self, cfg: &Config) -> Result<(), String> {
        if self.docs.is_empty() {
            return Err("docs cannot be empty".to_string());
        }
        if self.docs.len() > MAX_PREPARED_SESSION_DOCS {
            return Err(format!(
                "too many prepared session docs: {} > {}",
                self.docs.len(),
                MAX_PREPARED_SESSION_DOCS
            ));
        }
        if let Some(collection) = &self.collection {
            validate_collection_name(collection)?;
        }

        let per_doc_limit = super::session_ingest_max_bytes_for_config(cfg);
        for doc in &self.docs {
            doc.validate(per_doc_limit)?;
        }
        Ok(())
    }

    pub(crate) fn into_session_docs(self, cfg: &Config) -> Result<Vec<super::SessionDoc>, String> {
        self.validate(cfg)?;
        let collection = self.collection.clone();
        self.docs
            .into_iter()
            .map(|doc| {
                let resolved_collection = collection
                    .clone()
                    .unwrap_or_else(|| super::resolve_collection(cfg, &doc.collection_stem()));
                Ok(super::SessionDoc {
                    doc: doc.to_prepared_doc()?,
                    collection: resolved_collection,
                    raw_text: doc.text,
                })
            })
            .collect()
    }
}

impl PreparedSessionDoc {
    pub fn validate(&self, per_doc_limit: usize) -> Result<(), String> {
        if self.text.trim().is_empty() {
            return Err("prepared session text is empty".to_string());
        }
        if self.text.len() > per_doc_limit {
            return Err(format!(
                "prepared session text exceeds per-doc limit: {} > {}",
                self.text.len(),
                per_doc_limit
            ));
        }
        match self.session_platform.as_str() {
            "claude" | "codex" | "gemini" => {}
            other => return Err(format!("unsupported session_platform: {other}")),
        }
        if !self.url.starts_with("file://") {
            return Err("prepared session url must use file://".to_string());
        }
        if self.session_file.trim().is_empty() {
            return Err("session_file is required".to_string());
        }
        let metadata_bytes = serde_json::to_vec(&self.extra)
            .map_err(|err| format!("invalid extra metadata: {err}"))?
            .len();
        if metadata_bytes > MAX_PREPARED_SESSION_METADATA_BYTES {
            return Err(format!(
                "prepared session metadata exceeds limit: {metadata_bytes} > {MAX_PREPARED_SESSION_METADATA_BYTES}"
            ));
        }
        Ok(())
    }

    pub(crate) fn to_prepared_doc(&self) -> Result<PreparedDoc, String> {
        self.validate(usize::MAX)?;
        let source_type = match self.session_platform.as_str() {
            "claude" => "claude_session",
            "codex" => "codex_session",
            "gemini" => "gemini_session",
            other => return Err(format!("unsupported session_platform: {other}")),
        };
        let mut extra = serde_json::Map::new();
        if let Some(obj) = self.extra.as_object() {
            for (key, value) in obj {
                if !RESERVED_EXTRA_KEYS.contains(&key.as_str()) {
                    extra.insert(key.clone(), value.clone());
                }
            }
        }
        extra.insert(
            "agent".to_string(),
            serde_json::Value::String(self.session_platform.clone()),
        );
        extra.insert(
            "project_name".to_string(),
            self.session_project
                .clone()
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
        extra.insert(
            "session_date".to_string(),
            self.session_date
                .clone()
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
        extra.insert(
            "turn_count".to_string(),
            self.session_turn_count
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null),
        );
        extra.insert(
            "session_file".to_string(),
            serde_json::Value::String(self.session_file.clone()),
        );

        Ok(prepare_plain_text_source(
            self.url.clone(),
            url_to_domain(&self.url),
            self.text.clone(),
            source_type,
            self.title.clone(),
            Some(serde_json::Value::Object(extra)),
        ))
    }

    fn collection_stem(&self) -> String {
        self.session_project
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(self.session_platform.as_str())
            .to_string()
    }
}

/// Split prepared session docs into batches that each satisfy the per-request
/// limits enforced by [`IngestSessionsPreparedRequest::validate`] (doc count and
/// cumulative text size). A single oversized doc is left in its own batch — its
/// per-doc validation is the caller's responsibility.
pub(crate) fn split_prepared_session_docs(
    docs: Vec<PreparedSessionDoc>,
    cfg: &Config,
) -> Vec<Vec<PreparedSessionDoc>> {
    let total_limit = super::session_ingest_max_bytes_for_config(cfg).saturating_mul(4);
    let mut batches: Vec<Vec<PreparedSessionDoc>> = Vec::new();
    let mut current: Vec<PreparedSessionDoc> = Vec::new();
    let mut current_bytes = 0usize;
    for doc in docs {
        let doc_bytes = doc.text.len();
        let exceeds_count = current.len() >= MAX_PREPARED_SESSION_DOCS;
        let exceeds_bytes =
            !current.is_empty() && current_bytes.saturating_add(doc_bytes) > total_limit;
        if exceeds_count || exceeds_bytes {
            batches.push(std::mem::take(&mut current));
            current_bytes = 0;
        }
        current_bytes = current_bytes.saturating_add(doc_bytes);
        current.push(doc);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

fn validate_collection_name(value: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if valid {
        Ok(())
    } else {
        Err("collection must be 1-128 chars of ASCII letters, digits, '_', '-', or '.'".to_string())
    }
}
