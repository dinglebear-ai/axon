use std::fmt;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    LocalCode,
    Crawl,
    Git,
    Feed,
    Session,
    Media,
}

impl SourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LocalCode => "local_code",
            Self::Crawl => "crawl",
            Self::Git => "git",
            Self::Feed => "feed",
            Self::Session => "session",
            Self::Media => "media",
        }
    }
}

impl fmt::Display for SourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<&str> for SourceKind {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "local_code" => Ok(Self::LocalCode),
            "crawl" => Ok(Self::Crawl),
            "git" => Ok(Self::Git),
            "feed" => Ok(Self::Feed),
            "session" => Ok(Self::Session),
            "media" => Ok(Self::Media),
            other => anyhow::bail!("unknown source kind {other}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceIdentity {
    pub source_id: String,
    pub source_kind: SourceKind,
    pub collection: String,
    pub index_version: i64,
}

impl SourceIdentity {
    pub fn new(
        source_id: impl Into<String>,
        source_kind: SourceKind,
        collection: impl Into<String>,
        index_version: i64,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            source_kind,
            collection: collection.into(),
            index_version,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestItem {
    pub item_key: String,
    pub content_hash: String,
    pub size_bytes: i64,
}

impl ManifestItem {
    pub fn new(
        item_key: impl Into<String>,
        content_hash: impl Into<String>,
        size_bytes: i64,
    ) -> Self {
        Self {
            item_key: item_key.into(),
            content_hash: content_hash.into(),
            size_bytes,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ManifestDiff {
    pub added: Vec<ManifestItem>,
    pub modified: Vec<ManifestItem>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshPreflight {
    Ready,
    BackingOff {
        until_ms: i64,
        dependency: String,
        message: String,
    },
}
