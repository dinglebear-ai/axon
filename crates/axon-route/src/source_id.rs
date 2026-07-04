//! Stable source identity construction.

use axon_api::{SourceId, SourceKind};
use sha2::{Digest, Sha256};

pub fn source_id(source_kind: SourceKind, canonical_uri: &str) -> SourceId {
    SourceId::new(format!(
        "src_{}",
        stable_hash(&format!(
            "{}:{canonical_uri}:v1",
            source_kind_key(source_kind)
        ))[..16]
            .to_string()
    ))
}

pub fn local_project_key(path: &str) -> String {
    format!("lp_{}", &stable_hash(path)[..16])
}

pub fn stable_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

fn source_kind_key(source_kind: SourceKind) -> &'static str {
    match source_kind {
        SourceKind::Web => "web",
        SourceKind::Local => "local",
        SourceKind::Git => "git",
        SourceKind::Registry => "registry",
        SourceKind::Feed => "feed",
        SourceKind::Reddit => "reddit",
        SourceKind::Youtube => "youtube",
        SourceKind::Session => "session",
        SourceKind::CliTool => "cli_tool",
        SourceKind::McpTool => "mcp_tool",
        SourceKind::Memory => "memory",
        SourceKind::Upload => "upload",
    }
}
