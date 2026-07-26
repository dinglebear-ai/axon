use std::path::PathBuf;

use anyhow::{Result, bail};
use regex::Regex;
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
        let path = path.into();
        validate_relative_path(&path)?;
        if content.trim().is_empty() || is_header_only(&content) {
            bail!("docs generator produced an empty/header-only artifact");
        }
        validate_public_content(&content)?;
        let checksum = format!("sha256:{:x}", Sha256::digest(content.as_bytes()));
        Ok(Self {
            path,
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

fn validate_relative_path(path: &std::path::Path) -> Result<()> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::RootDir
            )
        })
    {
        bail!(
            "docs generator produced an unsafe artifact path: {}",
            path.display()
        );
    }
    Ok(())
}

fn validate_public_content(content: &str) -> Result<()> {
    let secret_patterns = [
        r"(?i)bearer\s+[a-z0-9._~+/=-]{12,}",
        r"\b(?:sk|ghp|github_pat|xox[baprs]-)[a-z0-9_-]{8,}",
        r"(?i)\b(?:api[_-]?key|client[_-]?secret|password|passwd|private[_-]?key|refresh[_-]?token)\s*[:=]\s*[^`\s|]+",
    ];
    for pattern in secret_patterns {
        if Regex::new(pattern)?.is_match(content) {
            bail!("docs generator output contains a secret-shaped value");
        }
    }

    let absolute_path =
        Regex::new(r"(?x)(?:^|[\s(`\[]) /(?:home|Users|private|tmp|var|etc|workspace|mnt)(?:/|$)")?;
    if absolute_path.is_match(content) {
        bail!("docs generator output contains an absolute filesystem path");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocsArtifactSet {
    pub family: super::families::DocsFamily,
    pub artifacts: Vec<GeneratedDocArtifact>,
    pub source_inputs: Vec<super::families::SourceInput>,
}
