use std::path::PathBuf;

use anyhow::{Result, bail};
use sha2::{Digest, Sha256};

/// A complete generated document.  The renderer owns the body as well as the
/// header so drift checks cannot accidentally compare a header to itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedDocArtifact {
    pub path: PathBuf,
    pub content: String,
    pub checksum: String,
    pub generated_by: &'static str,
}

impl GeneratedDocArtifact {
    pub fn new(
        path: impl Into<PathBuf>,
        content: String,
        generated_by: &'static str,
    ) -> Result<Self> {
        if content.trim().is_empty() || is_header_only(&content) {
            bail!("docs generator produced an empty/header-only artifact");
        }
        let checksum = format!("sha256:{:x}", Sha256::digest(content.as_bytes()));
        Ok(Self {
            path: path.into(),
            content,
            checksum,
            generated_by,
        })
    }
}

fn is_header_only(content: &str) -> bool {
    content
        .lines()
        .all(|line| line.trim().is_empty() || line.trim_start().starts_with("<!--"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocsArtifactSet {
    pub family: super::families::DocsFamily,
    pub artifacts: Vec<GeneratedDocArtifact>,
    pub source_inputs: Vec<super::families::SourceInput>,
}
