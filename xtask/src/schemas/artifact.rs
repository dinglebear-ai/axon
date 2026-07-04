use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaArtifact {
    pub path: PathBuf,
    pub content: String,
}

impl SchemaArtifact {
    pub fn new(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
        }
    }
}
