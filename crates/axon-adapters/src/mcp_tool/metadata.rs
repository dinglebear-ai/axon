//! MCP tool `SourceDocument` construction. Stamps only approved `tool`
//! payload-family metadata fields plus shared vector payload fields. Mirrors
//! `cli_tool/metadata.rs`.

use axon_api::source::*;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::McpToolTarget;

/// `tool_name` packs `"{server}/{tool}"` since the shared `tool` payload
/// family has no dedicated MCP-server field of its own. `tool_action` is a
/// fixed enum-shaped label (`"metadata"` vs `"call"`), not free text.
pub(super) fn mcp_tool_source_document(
    plan: &SourcePlan,
    acquisition: &SourceAcquisition,
    item: &AcquiredSourceItem,
    target: &McpToolTarget,
    tool_action: &str,
) -> SourceDocument {
    let mut metadata = MetadataMap::new();
    metadata.insert("source_family".to_string(), json!("tool"));
    metadata.insert("source_kind".to_string(), json!("mcp_tool"));
    metadata.insert("source_adapter".to_string(), json!(plan.route.adapter.name));
    metadata.insert("source_scope".to_string(), json!(plan.route.scope));
    metadata.insert(
        "tool_name".to_string(),
        json!(format!("{}/{}", target.server, target.tool)),
    );
    metadata.insert("tool_action".to_string(), json!(tool_action));
    metadata.insert("tool_side_effect_class".to_string(), json!("none"));
    metadata.insert(
        "item_canonical_uri".to_string(),
        json!(item.manifest_item.canonical_uri),
    );
    metadata.insert("committed_generation".to_string(), json!("uncommitted"));
    metadata.insert("visibility".to_string(), json!("internal"));
    let redaction_status = item
        .metadata
        .0
        .get("redaction_status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("clean");
    metadata.insert("redaction_status".to_string(), json!(redaction_status));
    if let Some(artifact_id) = item.raw_artifact_id.as_ref() {
        metadata.insert("tool_output_artifact_id".to_string(), json!(artifact_id.0));
    }
    SourceDocument {
        document_id: mcp_tool_document_id(
            &acquisition.source_id,
            &item.manifest_item.source_item_key,
        ),
        source_id: acquisition.source_id.clone(),
        source_item_key: item.manifest_item.source_item_key.clone(),
        canonical_uri: item.manifest_item.canonical_uri.clone(),
        content_kind: item
            .manifest_item
            .content_kind
            .unwrap_or(ContentKind::Structured),
        content: item.content_ref.clone(),
        metadata,
        title: item.manifest_item.display_path.clone(),
        language: None,
        path: item.manifest_item.display_path.clone(),
        mime_type: None,
        structured_payload: None,
        artifact_id: item.raw_artifact_id.clone(),
        chunk_hints: plan.route.chunking_hints.clone(),
        parser_hints: plan.route.parser_hints.clone(),
    }
}

fn mcp_tool_document_id(source_id: &SourceId, item_key: &SourceItemKey) -> DocumentId {
    DocumentId::from(format!(
        "doc_mcp_tool_{}",
        stable_token(&format!("{}\0{}", source_id.0, item_key.0))
    ))
}

fn stable_token(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let mut token = String::with_capacity(24);
    for byte in &digest[..12] {
        use std::fmt::Write as _;
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}
