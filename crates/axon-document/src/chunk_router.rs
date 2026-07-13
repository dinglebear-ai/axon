//! Route source documents to the PR8 chunking profiles.

use std::str::FromStr;

#[cfg(test)]
use axon_api::source::ChunkProfile;
#[cfg(test)]
use axon_api::source::ContentRef;
use axon_api::source::{ContentKind, MetadataMap, SourceDocument};

use crate::profile::ChunkingProfile;

/// Suggested token budget for a routed profile. Advisory: chunk builders are
/// not required to hit these exactly, but should treat them as soft targets
/// (`docs/pipeline-unification/sources/chunking-contract.md` "Size and
/// Overlap Rules").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkLimits {
    pub max_chunk_tokens: u32,
    pub overlap_tokens: u32,
}

/// Full routing decision: not just *which* profile, but *how* it should be
/// chunked (method, parser family, ordered fallback chain, size limits).
/// Contract shape: `docs/pipeline-unification/sources/chunking-contract.md`
/// `## ChunkRouter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteDecision {
    pub profile: ChunkingProfile,
    pub method: &'static str,
    pub parser_family: &'static str,
    pub fallback_chain: Vec<&'static str>,
    pub limits: ChunkLimits,
}

/// Byte threshold above which a profile's preferred method is likely to need
/// its first fallback step (huge documents cost more to parse precisely and
/// are more likely to blow structural parsers).
pub(crate) const LARGE_DOCUMENT_BYTES: usize = 200_000;

/// Adapters that hand `axon-document` fragments rather than whole, structurally
/// intact files: chat/tool payloads, web-scraped/rendered text, and other
/// non-source-controlled captures. Structural parsers (tree-sitter, markdown
/// heading walkers) are tuned for complete, well-formed documents and are
/// more likely to mis-parse a fragment than a size-based fallback would be
/// wrong about a huge-but-intact file, so these adapters skip straight to the
/// first fallback step in the chain regardless of size.
/// (`docs/pipeline-unification/sources/chunking-contract.md` "Routing order"
/// item 4, "Source adapter defaults".)
const FRAGMENT_PRONE_ADAPTERS: &[&str] = &[
    "web_scrape",
    "chrome_extension",
    "chat",
    "chat_transcript",
    "api_response",
    "reddit",
];

/// Source scopes that denote a partial/incremental capture (a diff, a single
/// changed record, a streamed fragment) rather than a full document. Partial
/// captures are already small, semantically-bounded units, so their chunk
/// budget is tightened rather than left at the full-document default -- a
/// half-page diff chunked at the same 1200-1600 token ceiling as a full file
/// would rarely split at all, defeating the purpose of a size limit.
const PARTIAL_SCOPES: &[&str] = &["diff", "partial", "fragment", "incremental"];

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

    /// Full routing decision for `doc`: profile plus method, parser family,
    /// fallback chain, and size limits. Considers the source adapter/scope
    /// (read from the shared metadata envelope, per `metadata-payload.md`)
    /// and the normalized document size, not just content kind.
    #[cfg(test)]
    pub(crate) fn route_decision(&self, doc: &SourceDocument) -> Result<RouteDecision, String> {
        let profile = self.route(doc)?;
        Ok(decision_for_profile(
            profile,
            document_size_bytes(doc),
            source_adapter(doc),
            source_scope(doc),
        ))
    }
}

#[cfg(test)]
fn document_size_bytes(doc: &SourceDocument) -> usize {
    match &doc.content {
        ContentRef::InlineText { text } => text.len(),
        _ => 0,
    }
}

pub(crate) fn source_adapter(doc: &SourceDocument) -> Option<&str> {
    doc.metadata
        .get("source_adapter")
        .and_then(serde_json::Value::as_str)
}

pub(crate) fn source_scope(doc: &SourceDocument) -> Option<&str> {
    doc.metadata
        .get("source_scope")
        .and_then(serde_json::Value::as_str)
}

/// Per-profile method/parser-family/fallback-chain/limits, adjusted for
/// document size, source adapter, and source scope
/// (`docs/pipeline-unification/sources/chunking-contract.md` "ChunkRouter"
/// inputs list; routing order items 4-5, "Source adapter defaults" then
/// "Size-based fallback"). Adapter is checked first since a fragment-prone
/// adapter forces the fallback method regardless of size; scope narrows the
/// token/overlap limits for partial captures independent of both.
/// The primary parser family, fallback chain, and token limits for each
/// chunking profile. Split out of `decision_for_profile` to keep it under the
/// monolith function-length cap.
fn profile_defaults(profile: ChunkingProfile) -> (&'static str, Vec<&'static str>, ChunkLimits) {
    match profile {
        ChunkingProfile::CodeSymbol => (
            "heuristic_symbol",
            vec!["ast_symbol_heuristic", "code_blocks", "line_window"],
            ChunkLimits {
                max_chunk_tokens: 1400,
                overlap_tokens: 90,
            },
        ),
        ChunkingProfile::CodeManifest => (
            "structured",
            vec!["structured_manifest", "atomic_metadata"],
            ChunkLimits {
                max_chunk_tokens: 1200,
                overlap_tokens: 0,
            },
        ),
        ChunkingProfile::MarkdownSections => (
            "markdown",
            vec!["heading_sections", "plain_text_windows"],
            ChunkLimits {
                max_chunk_tokens: 1600,
                overlap_tokens: 120,
            },
        ),
        ChunkingProfile::HtmlArticle => (
            "html",
            vec!["dom_to_markdown", "plain_text_windows"],
            ChunkLimits {
                max_chunk_tokens: 1600,
                overlap_tokens: 120,
            },
        ),
        ChunkingProfile::PlainTextWindows => (
            "plain_text",
            vec!["paragraph_windows", "line_window"],
            ChunkLimits {
                max_chunk_tokens: 1500,
                overlap_tokens: 140,
            },
        ),
        ChunkingProfile::TranscriptSegments => (
            "transcript",
            vec!["timestamp_turns", "line_segments"],
            ChunkLimits {
                max_chunk_tokens: 1300,
                overlap_tokens: 60,
            },
        ),
        ChunkingProfile::StructuredRecords => (
            "structured",
            vec!["structured_records", "atomic_metadata"],
            ChunkLimits {
                max_chunk_tokens: 1200,
                overlap_tokens: 40,
            },
        ),
        ChunkingProfile::ApiSchema => (
            "structured",
            vec!["schema_records", "structured_records", "atomic_metadata"],
            ChunkLimits {
                max_chunk_tokens: 1600,
                overlap_tokens: 60,
            },
        ),
        ChunkingProfile::ToolOutput => (
            "tool_output",
            vec!["command_records", "line_segments"],
            ChunkLimits {
                max_chunk_tokens: 1200,
                overlap_tokens: 60,
            },
        ),
        ChunkingProfile::SessionTurns => (
            "session",
            vec!["turn_segments", "line_segments"],
            ChunkLimits {
                max_chunk_tokens: 1300,
                overlap_tokens: 0,
            },
        ),
        ChunkingProfile::AtomicMetadata => (
            "atomic",
            vec!["atomic_metadata"],
            ChunkLimits {
                max_chunk_tokens: 1600,
                overlap_tokens: 0,
            },
        ),
    }
}

pub(crate) fn decision_for_profile(
    profile: ChunkingProfile,
    size_bytes: usize,
    adapter: Option<&str>,
    scope: Option<&str>,
) -> RouteDecision {
    let large = size_bytes > LARGE_DOCUMENT_BYTES;
    let fragment_prone = adapter.is_some_and(|adapter| FRAGMENT_PRONE_ADAPTERS.contains(&adapter));
    let partial_scope = scope.is_some_and(|scope| PARTIAL_SCOPES.contains(&scope));
    let (parser_family, fallback_chain, limits) = profile_defaults(profile);

    // A large document, or one from a fragment-prone adapter, is unlikely to
    // fit its preferred structural method cleanly; report the first fallback
    // step as the active method so observability reflects reality instead of
    // an aspirational default. Gated to the profiles whose chunk-building
    // dispatch (`preparer::build_chunks`) actually runs a distinct fallback
    // implementation for this trigger -- reporting a fallback method that no
    // code path executes would be the same disconnect this gate exists to
    // prevent (see `docs/reports/2026-07-09-pipeline-unification-alignment-audit.md`
    // S2-19).
    let method = if (large || fragment_prone)
        && fallback_chain.len() > 1
        && profile.has_wired_structural_fallback()
    {
        fallback_chain[1]
    } else {
        fallback_chain[0]
    };

    // A partial/diff/fragment scope is already a small, bounded unit; halve
    // the chunk budget (floor 200 tokens) so it still gets a meaningful split
    // instead of riding the full-document ceiling untouched.
    let limits = if partial_scope {
        ChunkLimits {
            max_chunk_tokens: (limits.max_chunk_tokens / 2).max(200),
            overlap_tokens: limits.overlap_tokens / 2,
        }
    } else {
        limits
    };

    RouteDecision {
        profile,
        method,
        parser_family,
        fallback_chain,
        limits,
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
