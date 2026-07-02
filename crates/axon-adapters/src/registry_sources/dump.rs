//! Registry dump JSON schema — the pure input contract for the registry
//! adapter. A "dump" is a prepared local JSON file describing one package's
//! metadata (name, versions, readme, description). Fetching that dump from a
//! live registry API (npm/PyPI/crates.io/etc.) is the bridge's job, not this
//! adapter's — see `crates/axon-adapters/src/CLAUDE.md`.

use std::path::Path;

use axon_api::source::ApiError;

use crate::adapter::Result;

/// One package's registry metadata, as produced by a bridge fetch and read
/// from disk via the `registry_dump_path` adapter option.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RegistryDump {
    /// Registry family: `"npm"`, `"pypi"`, `"crates"`, etc.
    pub registry: String,
    /// Package name as known to the registry.
    pub package: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Versions available for this package. At least one version is
    /// required — the dump is malformed without it.
    pub versions: Vec<RegistryDumpVersion>,
}

/// One version of a package within a [`RegistryDump`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RegistryDumpVersion {
    pub version: String,
    #[serde(default)]
    pub readme: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub is_latest: bool,
}

impl RegistryDump {
    /// Load and validate a registry dump from `path`.
    ///
    /// Pure and synchronous: no network calls, filesystem read only.
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path).map_err(|err| {
            ApiError::new(
                "adapter.registry.dump_unreadable",
                axon_error::ErrorStage::Discovering,
                format!("failed to read registry dump: {err}"),
            )
            .with_context("path", path.display().to_string())
        })?;
        Self::parse(&raw)
    }

    /// Parse and validate a registry dump from a JSON string.
    pub fn parse(raw: &str) -> Result<Self> {
        let dump: Self = serde_json::from_str(raw).map_err(|err| {
            ApiError::new(
                "adapter.registry.dump_malformed",
                axon_error::ErrorStage::Discovering,
                format!("failed to parse registry dump JSON: {err}"),
            )
        })?;
        dump.validate()?;
        Ok(dump)
    }

    fn validate(&self) -> Result<()> {
        if self.registry.trim().is_empty() {
            return Err(ApiError::new(
                "adapter.registry.dump_invalid",
                axon_error::ErrorStage::Discovering,
                "registry dump is missing a registry name",
            ));
        }
        if self.package.trim().is_empty() {
            return Err(ApiError::new(
                "adapter.registry.dump_invalid",
                axon_error::ErrorStage::Discovering,
                "registry dump is missing a package name",
            ));
        }
        if self.versions.is_empty() {
            return Err(ApiError::new(
                "adapter.registry.dump_invalid",
                axon_error::ErrorStage::Discovering,
                "registry dump must declare at least one version",
            ));
        }
        for version in &self.versions {
            if version.version.trim().is_empty() {
                return Err(ApiError::new(
                    "adapter.registry.dump_invalid",
                    axon_error::ErrorStage::Discovering,
                    "registry dump version entry is missing a version string",
                ));
            }
        }
        Ok(())
    }

    /// The version flagged `is_latest`, falling back to the last entry in
    /// declaration order when none is flagged.
    pub fn latest_version(&self) -> Option<&RegistryDumpVersion> {
        self.versions
            .iter()
            .find(|version| version.is_latest)
            .or_else(|| self.versions.last())
    }

    /// Look up a specific version by its version string.
    pub fn version(&self, version: &str) -> Option<&RegistryDumpVersion> {
        self.versions.iter().find(|entry| entry.version == version)
    }
}

#[cfg(test)]
#[path = "../registry_sources_dump_tests.rs"]
mod tests;
