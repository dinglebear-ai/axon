//! [`SessionDoc`]: the raw ingredients for one session transcript, and its
//! conversion into a contract [`PreparedDocument`] at embed time.
//!
//! Split out of `sessions_legacy.rs` to keep that file under the repo's
//! 500-line monolith cap.

use crate::contract_write;
use axon_api::source::{
    ChunkHint, ContentKind, ContentRef, DocumentId, MetadataMap, ParserHint, PreparedDocument,
    SourceDocument, SourceId, SourceItemKey, SourceScope,
};

/// Source-family-specific fields the vector payload contract allows for the
/// `"session"` family
/// (`axon_vectors::payload_families::VECTOR_SOURCE_FAMILY_FIELDS`). Kept in
/// sync with `sessions_source::sessions_source_adapter::SESSION_PAYLOAD_ALLOWED_FIELDS`.
/// The legacy path's much richer `extra` metadata (project name, session
/// date, turn count, model, tool usage, workspace path, git branch, gh repo)
/// has no slot in this allowlist and is dropped from the vector payload — the
/// same accepted limitation documented in `crate::contract_write` and already
/// applied by the ledger-backed `sessions_source` bridge.
const SESSION_PAYLOAD_ALLOWED_FIELDS: &[&str] = &["session_id"];

/// A parsed session document ready for preparation + embedding.
///
/// Deliberately holds the raw ingredients (not a pre-chunked/prepared
/// document): [`SessionDoc::to_prepared_document`] builds the contract
/// `SourceDocument` and runs it through `DocumentPreparer` lazily, at embed
/// time, so the same ingredients also serve the
/// `/v1/ingest/sessions/prepared` wire round-trip
/// (`prepared_session_doc_from_session_doc`) without needing to reverse a
/// chunked document back into its source fields.
pub(crate) struct SessionDoc {
    pub(crate) url: String,
    pub(crate) title: Option<String>,
    pub(crate) source_type: &'static str,
    pub(crate) extra: Option<serde_json::Value>,
    pub(crate) collection: String,
    pub(crate) raw_text: String,
}

impl SessionDoc {
    /// Build the contract `SourceDocument` (`source_family = "session"`) and
    /// run it through `DocumentPreparer`. `ContentKind::PlainText` mirrors the
    /// legacy `prepare_plain_text_source`'s chunk-text-with-offsets chunker
    /// (routed here via `ChunkingProfile::PlainTextWindows`).
    pub(super) fn to_prepared_document(&self) -> Result<PreparedDocument, String> {
        let token = contract_write::stable_token(&format!("session:{}", self.url));
        let mut metadata = MetadataMap::new();
        metadata.insert("source_family".to_string(), serde_json::json!("session"));
        metadata.insert(
            "source_type".to_string(),
            serde_json::json!(self.source_type),
        );
        metadata.insert("source_kind".to_string(), serde_json::json!("session"));
        metadata.insert(
            "source_adapter".to_string(),
            serde_json::json!("session_legacy"),
        );
        metadata.insert(
            "source_scope".to_string(),
            serde_json::json!(SourceScope::File),
        );
        if let Some(session_id) = self
            .extra
            .as_ref()
            .and_then(|extra| extra.get("session_id"))
            .and_then(|value| value.as_str())
        {
            metadata.insert("session_id".to_string(), serde_json::json!(session_id));
        }
        contract_write::retain_contract_fields(&mut metadata, SESSION_PAYLOAD_ALLOWED_FIELDS);

        let document = SourceDocument {
            document_id: DocumentId::new(format!("doc_session_{token}")),
            source_id: SourceId::new(format!("src_session_{token}")),
            source_item_key: SourceItemKey::new(self.url.clone()),
            canonical_uri: self.url.clone(),
            content_kind: ContentKind::PlainText,
            content: ContentRef::InlineText {
                text: self.raw_text.clone(),
            },
            metadata,
            title: self.title.clone(),
            language: None,
            path: None,
            mime_type: None,
            structured_payload: None,
            artifact_id: None,
            chunk_hints: Vec::<ChunkHint>::new(),
            parser_hints: Vec::<ParserHint>::new(),
        };
        contract_write::prepare_document(document, contract_write::adhoc_generation())
    }
}
