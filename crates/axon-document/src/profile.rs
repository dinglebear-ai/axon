//! Chunking profile names owned by the document preparation layer.

use std::fmt;
use std::str::FromStr;

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
