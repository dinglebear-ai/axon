use serde_json::Value;

const PLANNER_OWNED_PAYLOAD_KEYS: &[&str] = &[
    "content_kind",
    "chunk_content_kind",
    "chunk_locator",
    "source_range",
    "chunking_fallback",
    "code_chunk_source",
];

const LEDGER_OWNED_EXTRA_KEYS: &[&str] = &[
    "source_id",
    "source_kind",
    "source_generation",
    "source_item_key",
    "source_item_hash",
    "source_index_version",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerPayload {
    source_id: String,
    source_kind: String,
    generation: i64,
    item_key: String,
    index_version: i64,
}

impl LedgerPayload {
    pub fn try_new(
        source_id: String,
        source_kind: impl Into<String>,
        generation: i64,
        item_key: String,
        index_version: i64,
    ) -> Result<Self, String> {
        let source_kind = source_kind.into();
        if source_id.trim().is_empty() {
            return Err("ledger source_id cannot be empty".to_string());
        }
        if source_kind.trim().is_empty() {
            return Err("ledger source_kind cannot be empty".to_string());
        }
        if generation <= 0 {
            return Err("ledger generation must be positive".to_string());
        }
        if item_key.trim().is_empty() {
            return Err("ledger item_key cannot be empty".to_string());
        }
        if index_version <= 0 {
            return Err("ledger index_version must be positive".to_string());
        }
        Ok(Self {
            source_id,
            source_kind,
            generation,
            item_key,
            index_version,
        })
    }

    pub(in crate::ops) fn apply_to_payload(&self, payload: &mut Value) {
        payload["source_id"] = Value::String(self.source_id.clone());
        payload["source_kind"] = Value::String(self.source_kind.clone());
        payload["source_generation"] = Value::Number(self.generation.into());
        payload["source_item_key"] = Value::String(self.item_key.clone());
        payload["source_index_version"] = Value::Number(self.index_version.into());
    }
}

pub(super) fn sanitize_doc_extra(extra: Option<Value>) -> Result<Option<Value>, String> {
    match extra {
        Some(Value::Object(mut map)) => {
            if let Some(key) = LEDGER_OWNED_EXTRA_KEYS
                .iter()
                .find(|key| map.contains_key(**key))
            {
                return Err(format!(
                    "ledger-owned payload key `{key}` cannot be set via extra"
                ));
            }
            for key in PLANNER_OWNED_PAYLOAD_KEYS {
                map.remove(*key);
            }
            Ok(Some(Value::Object(map)))
        }
        other => Ok(other),
    }
}
