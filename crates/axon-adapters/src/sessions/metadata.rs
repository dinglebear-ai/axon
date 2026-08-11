//! Session `SourceDocument` construction — stamps only approved session
//! metadata fields onto normalized documents. Mirrors `git/metadata.rs`.

use axon_api::source::*;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::decode::DecodedSession;
use super::hex_prefix;
use super::target::SessionTarget;

pub(super) fn session_source_document(
    plan: &SourcePlan,
    target: &SessionTarget,
    decoded: &DecodedSession,
    acquisition: &SourceAcquisition,
    item: &AcquiredSourceItem,
) -> SourceDocument {
    let document_id =
        session_document_id(&acquisition.source_id, &item.manifest_item.source_item_key);
    let provider = target.provider.as_str();
    let safe_session_id = opaque_session_id(provider, &target.session_id);
    let canonical_uri = format!("session://{provider}/{}", document_id.0);

    let mut metadata = MetadataMap::new();
    metadata.insert("source_family".to_string(), json!("session"));
    metadata.insert("source_kind".to_string(), json!("session"));
    metadata.insert("source_adapter".to_string(), json!(plan.route.adapter.name));
    metadata.insert("source_scope".to_string(), json!(plan.route.scope));
    metadata.insert("session_provider".to_string(), json!(target.provider));
    metadata.insert("session_agent".to_string(), json!(target.provider));
    // Vector payloads pass through the public redaction boundary. Raw local
    // session paths/IDs there would be scrubbed and correctly stamped
    // `redacted`, which normal retrieval excludes. Keep raw transport identity
    // in the manifest/artifact boundary and project only stable opaque IDs into
    // the retrievable document.
    metadata.insert("session_id".to_string(), json!(safe_session_id));
    metadata.insert("session_turn_count".to_string(), json!(decoded.turn_count));
    metadata.insert(
        "session_has_tool_use".to_string(),
        json!(decoded.has_tool_use),
    );
    metadata.insert("session_tools_used".to_string(), json!(decoded.tools_used));
    if let Some(model) = &decoded.model {
        metadata.insert("session_model".to_string(), json!(model));
    }
    if let Some(workspace_path) = &decoded.workspace_path {
        metadata.insert("session_workspace_path".to_string(), json!(workspace_path));
    }
    if let Some(git_branch) = &decoded.git_branch {
        metadata.insert("session_git_branch".to_string(), json!(git_branch));
    }
    if let Some(last_message_at) = &decoded.last_message_at {
        metadata.insert(
            "session_last_message_at".to_string(),
            json!(last_message_at),
        );
    }
    metadata.insert(
        "item_canonical_uri".to_string(),
        json!(item.manifest_item.canonical_uri),
    );
    metadata.insert("committed_generation".to_string(), json!("uncommitted"));
    metadata.insert("visibility".to_string(), json!("internal"));
    // Provider decoders project only semantic turn text and run every turn
    // through `redact_session_text` before this document reaches the shared
    // preparation/vector boundary. `redaction_status = redacted` means the
    // payload is still unsafe for normal retrieval; the retrieval engine
    // intentionally filters to `clean`. This normalized representation is
    // safe to retrieve, while the acquired raw transport remains provenance
    // at the source/artifact boundary.
    metadata.insert("redaction_status".to_string(), json!("clean"));
    // The session adapter owns its payload projection. Keep only the
    // canonical source fields and the explicitly supported session metadata;
    // the shared runner must not need a family-specific cleanup branch.
    metadata.retain(|key, _| {
        matches!(
            key.as_str(),
            "source_family"
                | "source_kind"
                | "source_adapter"
                | "source_scope"
                | "session_provider"
                | "session_id"
                | "session_turn_index"
                | "session_tool_name"
                | "session_skill_name"
                | "committed_generation"
                | "visibility"
                | "redaction_status"
        )
    });

    let chunk_hints = if plan.route.chunking_hints.is_empty() {
        vec![ChunkHint {
            profile: ChunkProfile::SessionTurns,
            reason: "decoded session semantic turns".to_string(),
            options: MetadataMap::new(),
        }]
    } else {
        plan.route.chunking_hints.clone()
    };

    SourceDocument {
        document_id,
        source_id: acquisition.source_id.clone(),
        source_item_key: item.manifest_item.source_item_key.clone(),
        canonical_uri,
        // The acquired artifact is JSONL/JSON transcript transport, but the
        // normalized body below is semantic plain text. Keeping the raw
        // Transcript kind/path here re-selected the JSONL parser and emitted
        // an invalid-line warning for every decoded turn.
        content_kind: ContentKind::PlainText,
        content: ContentRef::InlineText {
            text: decoded.text.clone(),
        },
        metadata,
        title: Some(format!("{provider} AI session")),
        language: None,
        path: None,
        mime_type: None,
        structured_payload: None,
        artifact_id: item.raw_artifact_id.clone(),
        chunk_hints,
        parser_hints: Vec::new(),
    }
}

fn opaque_session_id(provider: &str, session_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("session-id\0{provider}\0{session_id}").as_bytes());
    format!("session_{}", hex_prefix(&hasher.finalize(), 24))
}

fn session_document_id(source_id: &SourceId, item_key: &SourceItemKey) -> DocumentId {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}\0{}", source_id.0, item_key.0).as_bytes());
    DocumentId::from(format!(
        "doc_session_{}",
        hex_prefix(&hasher.finalize(), 24)
    ))
}
