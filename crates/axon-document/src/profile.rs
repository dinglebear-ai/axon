//! Chunking profile names owned by the document preparation layer.

use std::fmt;
use std::str::FromStr;

use axon_api::source::ChunkProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChunkingProfile {
    CodeSymbol,
    CodeManifest,
    MarkdownSections,
    HtmlArticle,
    PlainTextWindows,
    TranscriptSegments,
    StructuredRecords,
    ApiSchema,
    ToolOutput,
    SessionTurns,
    AtomicMetadata,
}

impl From<ChunkProfile> for ChunkingProfile {
    fn from(value: ChunkProfile) -> Self {
        match value {
            ChunkProfile::CodeSymbol => Self::CodeSymbol,
            ChunkProfile::CodeManifest => Self::CodeManifest,
            ChunkProfile::MarkdownSections => Self::MarkdownSections,
            ChunkProfile::HtmlArticle => Self::HtmlArticle,
            ChunkProfile::PlainTextWindows => Self::PlainTextWindows,
            ChunkProfile::TranscriptSegments => Self::TranscriptSegments,
            ChunkProfile::StructuredRecords => Self::StructuredRecords,
            ChunkProfile::ApiSchema => Self::ApiSchema,
            ChunkProfile::ToolOutput => Self::ToolOutput,
            ChunkProfile::SessionTurns => Self::SessionTurns,
            ChunkProfile::AtomicMetadata => Self::AtomicMetadata,
        }
    }
}

/// Promoted from the former `#[cfg(test)]`-only `chunk_router::public_profiles()`
/// mapping — all 11 variants line up 1:1 with `axon_api::source::ChunkProfile`.
/// Used by `crate::boundary::ChunkRouter` to map the internal profile enum
/// back onto the contract's transport-neutral `ChunkProfile`.
impl From<ChunkingProfile> for ChunkProfile {
    fn from(value: ChunkingProfile) -> Self {
        match value {
            ChunkingProfile::CodeSymbol => Self::CodeSymbol,
            ChunkingProfile::CodeManifest => Self::CodeManifest,
            ChunkingProfile::MarkdownSections => Self::MarkdownSections,
            ChunkingProfile::HtmlArticle => Self::HtmlArticle,
            ChunkingProfile::PlainTextWindows => Self::PlainTextWindows,
            ChunkingProfile::TranscriptSegments => Self::TranscriptSegments,
            ChunkingProfile::StructuredRecords => Self::StructuredRecords,
            ChunkingProfile::ApiSchema => Self::ApiSchema,
            ChunkingProfile::ToolOutput => Self::ToolOutput,
            ChunkingProfile::SessionTurns => Self::SessionTurns,
            ChunkingProfile::AtomicMetadata => Self::AtomicMetadata,
        }
    }
}

impl ChunkingProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CodeSymbol => "code_symbol",
            Self::CodeManifest => "code_manifest",
            Self::MarkdownSections => "markdown_sections",
            Self::HtmlArticle => "html_article",
            Self::PlainTextWindows => "plain_text_windows",
            Self::TranscriptSegments => "transcript_segments",
            Self::StructuredRecords => "structured_records",
            Self::ApiSchema => "api_schema",
            Self::ToolOutput => "tool_output",
            Self::SessionTurns => "session_turns",
            Self::AtomicMetadata => "atomic_metadata",
        }
    }

    /// True for profiles where `preparer::build_chunks` actually dispatches to
    /// a distinct windowed-text implementation when the router flags a
    /// size/adapter fallback, instead of always running the primary
    /// structural chunker regardless of the router's decision. Used to gate
    /// `chunk_router::decision_for_profile`'s reported fallback method so it
    /// never claims a method that no code path executes.
    pub(crate) fn has_wired_structural_fallback(self) -> bool {
        matches!(
            self,
            Self::CodeSymbol | Self::MarkdownSections | Self::HtmlArticle
        )
    }
}

impl fmt::Display for ChunkingProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ChunkingProfile {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "code_symbol" => Ok(Self::CodeSymbol),
            "code_manifest" => Ok(Self::CodeManifest),
            "markdown_sections" => Ok(Self::MarkdownSections),
            "html_article" => Ok(Self::HtmlArticle),
            "plain_text_windows" => Ok(Self::PlainTextWindows),
            "transcript_segments" => Ok(Self::TranscriptSegments),
            "structured_records" => Ok(Self::StructuredRecords),
            "api_schema" => Ok(Self::ApiSchema),
            "tool_output" => Ok(Self::ToolOutput),
            "session_turns" => Ok(Self::SessionTurns),
            "atomic_metadata" => Ok(Self::AtomicMetadata),
            other => Err(format!("unknown chunking profile: {other}")),
        }
    }
}
