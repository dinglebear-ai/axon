//! Internal prepared chunk fragments before they become API DTO chunks.

use axon_api::source::{MetadataMap, SourceRange};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DocumentChunk {
    pub(crate) content: String,
    pub(crate) title: Option<String>,
    pub(crate) heading_path: Vec<String>,
    pub(crate) symbol: Option<String>,
    pub(crate) range: SourceRange,
    pub(crate) metadata: MetadataMap,
}

impl DocumentChunk {
    pub(crate) fn new(content: impl Into<String>, range: SourceRange) -> Self {
        Self {
            content: content.into(),
            title: None,
            heading_path: Vec::new(),
            symbol: None,
            range,
            metadata: MetadataMap::new(),
        }
    }

    pub(crate) fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub(crate) fn with_heading_path(mut self, heading_path: Vec<String>) -> Self {
        self.heading_path = heading_path;
        self
    }

    pub(crate) fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }

    pub(crate) fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }
}
