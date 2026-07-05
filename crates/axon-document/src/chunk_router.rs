//! Route source documents to the PR8 chunking profiles.

use std::str::FromStr;

#[cfg(test)]
use axon_api::source::ChunkProfile;
use axon_api::source::{ContentKind, MetadataMap, SourceDocument};

use crate::profile::ChunkingProfile;

#[derive(Debug, Default, Clone, Copy)]
pub struct ChunkRouter;

impl ChunkRouter {
    pub fn route(&self, doc: &SourceDocument) -> Result<ChunkingProfile, String> {
        if let Some(profile) = explicit_profile(doc)? {
            return Ok(profile);
        }

        if is_api_schema(doc) {
            return Ok(ChunkingProfile::ApiSchema);
        }
        if is_tool_output(doc) {
            return Ok(ChunkingProfile::ToolOutput);
        }
        if is_session_turns(doc) {
            return Ok(ChunkingProfile::SessionTurns);
        }
        if is_env_example(doc) {
            return Ok(ChunkingProfile::StructuredRecords);
        }
        if is_manifest(doc) {
            return Ok(ChunkingProfile::CodeManifest);
        }

        Ok(match doc.content_kind {
            ContentKind::Code => ChunkingProfile::CodeSymbol,
            ContentKind::Markdown => ChunkingProfile::MarkdownSections,
            ContentKind::Html => ChunkingProfile::HtmlArticle,
            ContentKind::PlainText => ChunkingProfile::PlainTextWindows,
            ContentKind::Transcript => ChunkingProfile::TranscriptSegments,
            ContentKind::Structured | ContentKind::Json | ContentKind::Yaml | ContentKind::Xml => {
                ChunkingProfile::StructuredRecords
            }
            ContentKind::Toml => ChunkingProfile::CodeManifest,
            ContentKind::BinaryMetadata => ChunkingProfile::AtomicMetadata,
        })
    }
}

fn explicit_profile(doc: &SourceDocument) -> Result<Option<ChunkingProfile>, String> {
    if let Some(hint) = doc.chunk_hints.first() {
        return Ok(Some(hint.profile.clone().into()));
    }

    for map in
        std::iter::once(&doc.metadata).chain(doc.chunk_hints.iter().map(|hint| &hint.options))
    {
        if let Some(value) = profile_value(map) {
            return ChunkingProfile::from_str(value).map(Some);
        }
    }
    Ok(None)
}

#[cfg(test)]
pub(crate) fn public_profiles() -> [(ChunkProfile, ChunkingProfile); 11] {
    [
        (ChunkProfile::CodeSymbol, ChunkingProfile::CodeSymbol),
        (ChunkProfile::CodeManifest, ChunkingProfile::CodeManifest),
        (
            ChunkProfile::MarkdownSections,
            ChunkingProfile::MarkdownSections,
        ),
        (ChunkProfile::HtmlArticle, ChunkingProfile::HtmlArticle),
        (
            ChunkProfile::PlainTextWindows,
            ChunkingProfile::PlainTextWindows,
        ),
        (
            ChunkProfile::TranscriptSegments,
            ChunkingProfile::TranscriptSegments,
        ),
        (
            ChunkProfile::StructuredRecords,
            ChunkingProfile::StructuredRecords,
        ),
        (ChunkProfile::ApiSchema, ChunkingProfile::ApiSchema),
        (ChunkProfile::ToolOutput, ChunkingProfile::ToolOutput),
        (ChunkProfile::SessionTurns, ChunkingProfile::SessionTurns),
        (
            ChunkProfile::AtomicMetadata,
            ChunkingProfile::AtomicMetadata,
        ),
    ]
}

fn profile_value(map: &MetadataMap) -> Option<&str> {
    ["axon_document_profile", "chunking_profile"]
        .iter()
        .find_map(|key| map.get(*key).and_then(serde_json::Value::as_str))
}

fn is_manifest(doc: &SourceDocument) -> bool {
    doc.path
        .as_deref()
        .or_else(|| doc.canonical_uri.rsplit('/').next())
        .is_some_and(|path| {
            let filename = path.rsplit('/').next().unwrap_or(path);
            matches!(
                filename,
                "Cargo.toml"
                    | "package.json"
                    | "package-lock.json"
                    | "pnpm-lock.yaml"
                    | "yarn.lock"
                    | "requirements.txt"
                    | "pyproject.toml"
                    | "go.mod"
                    | "pom.xml"
                    | "Dockerfile"
                    | "docker-compose.yml"
                    | "docker-compose.yaml"
                    | "Chart.yaml"
                    | "values.yaml"
                    | "kustomization.yaml"
                    | "kustomization.yml"
            ) || filename.ends_with(".tf")
                || filename.ends_with(".tfvars")
        })
}

fn is_env_example(doc: &SourceDocument) -> bool {
    doc.path
        .as_deref()
        .or_else(|| doc.canonical_uri.rsplit('/').next())
        .is_some_and(|path| {
            let filename = path.rsplit('/').next().unwrap_or(path);
            matches!(
                filename,
                ".env.example"
                    | ".env.sample"
                    | ".env.template"
                    | "example.env"
                    | "env.example"
                    | "env.sample"
                    | "env.template"
            ) || filename.ends_with(".env.example")
        })
}

fn is_tool_output(doc: &SourceDocument) -> bool {
    doc.path
        .as_deref()
        .or_else(|| doc.canonical_uri.rsplit('/').next())
        .is_some_and(|path| path.rsplit('/').next().unwrap_or(path) == "tool-output.jsonl")
}

fn is_session_turns(doc: &SourceDocument) -> bool {
    doc.path
        .as_deref()
        .or_else(|| doc.canonical_uri.rsplit('/').next())
        .is_some_and(|path| path.rsplit('/').next().unwrap_or(path) == "session.jsonl")
}

fn is_api_schema(doc: &SourceDocument) -> bool {
    let path = doc.path.as_deref().unwrap_or(&doc.canonical_uri);
    path.contains("openapi")
        || path.contains("swagger")
        || path.ends_with(".graphql")
        || path.ends_with(".graphqls")
        || path.ends_with(".proto")
        || doc.mime_type.as_deref().is_some_and(|mime| {
            mime.contains("schema")
                || mime.contains("graphql")
                || mime.contains("protobuf")
                || mime.contains("proto")
        })
}
