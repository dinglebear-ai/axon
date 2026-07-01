//! Route source documents to the PR8 chunking profiles.

use std::str::FromStr;

use axon_api::source::{ContentKind, MetadataMap, SourceDocument};

use crate::profile::ChunkingProfile;

#[derive(Debug, Default, Clone, Copy)]
pub struct ChunkRouter;

impl ChunkRouter {
    pub fn route(&self, doc: &SourceDocument) -> Result<ChunkingProfile, String> {
        if let Some(profile) = explicit_profile(doc)? {
            return Ok(profile);
        }

        if is_manifest(doc) {
            return Ok(ChunkingProfile::CodeManifest);
        }
        if is_api_schema(doc) {
            return Ok(ChunkingProfile::ApiSchema);
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
    for map in
        std::iter::once(&doc.metadata).chain(doc.chunk_hints.iter().map(|hint| &hint.options))
    {
        if let Some(value) = profile_value(map) {
            return ChunkingProfile::from_str(value).map(Some);
        }
    }
    Ok(None)
}

fn profile_value(map: &MetadataMap) -> Option<&str> {
    ["axon_document_profile", "chunking_profile", "profile"]
        .iter()
        .find_map(|key| map.get(*key).and_then(serde_json::Value::as_str))
}

fn is_manifest(doc: &SourceDocument) -> bool {
    doc.path
        .as_deref()
        .or_else(|| doc.canonical_uri.rsplit('/').next())
        .is_some_and(|path| {
            matches!(
                path,
                "Cargo.toml" | "package.json" | "pyproject.toml" | "go.mod" | "pom.xml"
            )
        })
}

fn is_api_schema(doc: &SourceDocument) -> bool {
    let path = doc.path.as_deref().unwrap_or(&doc.canonical_uri);
    path.contains("openapi")
        || path.contains("swagger")
        || doc
            .mime_type
            .as_deref()
            .is_some_and(|mime| mime.contains("schema"))
}
