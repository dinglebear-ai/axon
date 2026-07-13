use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use super::SchemaFamily;
use super::artifact::SchemaArtifact;

#[derive(Debug, Clone)]
pub struct IndexedArtifact {
    #[allow(dead_code)]
    pub family: SchemaFamily,
    pub path: PathBuf,
    pub raw: String,
    pub json: Option<Value>,
}

#[derive(Debug, Default, Clone)]
pub struct ArtifactIndex {
    artifacts: BTreeMap<PathBuf, IndexedArtifact>,
}

impl ArtifactIndex {
    pub fn from_generated(
        family: SchemaFamily,
        artifacts: &[SchemaArtifact],
    ) -> Result<ArtifactIndex> {
        let mut index = ArtifactIndex::default();
        index.extend_generated(family, artifacts)?;
        Ok(index)
    }

    pub fn extend_generated(
        &mut self,
        family: SchemaFamily,
        artifacts: &[SchemaArtifact],
    ) -> Result<()> {
        for artifact in artifacts {
            let json = parse_json_artifact(&artifact.path, &artifact.content)?;
            self.artifacts.insert(
                artifact.path.clone(),
                IndexedArtifact {
                    family,
                    path: artifact.path.clone(),
                    raw: artifact.content.clone(),
                    json,
                },
            );
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn from_existing(
        root: &Path,
        family: SchemaFamily,
        artifacts: &[SchemaArtifact],
    ) -> Result<Self> {
        let mut index = ArtifactIndex::default();
        for artifact in artifacts {
            let path = root.join(&artifact.path);
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read artifact {}", artifact.path.display()))?;
            let json = parse_json_artifact(&artifact.path, &raw)?;
            index.artifacts.insert(
                artifact.path.clone(),
                IndexedArtifact {
                    family,
                    path: artifact.path.clone(),
                    raw,
                    json,
                },
            );
        }
        Ok(index)
    }

    #[allow(dead_code)]
    pub fn get(&self, path: impl AsRef<Path>) -> Option<&IndexedArtifact> {
        self.artifacts.get(path.as_ref())
    }

    #[allow(dead_code)]
    pub fn contains(&self, path: impl AsRef<Path>) -> bool {
        self.artifacts.contains_key(path.as_ref())
    }

    pub fn iter(&self) -> impl Iterator<Item = &IndexedArtifact> {
        self.artifacts.values()
    }
}

fn parse_json_artifact(path: &Path, content: &str) -> Result<Option<Value>> {
    if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
        return Ok(None);
    }
    serde_json::from_str(content)
        .with_context(|| format!("failed to parse generated JSON artifact {}", path.display()))
        .map(Some)
}
