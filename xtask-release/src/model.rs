//! The release data model: the `release/components.toml` schema plus the plan
//! and mode types the command surface exposes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum GateMode {
    Pr,
    Main,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum BumpLevel {
    Patch,
    Minor,
    Major,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseDriver {
    AxonNative,
    ReleasePlease,
}

impl ReleaseDriver {
    pub(crate) const fn is_axon_native(self) -> bool {
        matches!(self, Self::AxonNative)
    }

    pub(crate) const fn is_release_please(self) -> bool {
        matches!(self, Self::ReleasePlease)
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AxonNative => "axon-native",
            Self::ReleasePlease => "release-please",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentPlan {
    pub id: String,
    pub name: String,
    pub changed: bool,
    pub version: String,
    pub candidate_tag: String,
    pub last_tag: Option<String>,
    pub release_workflow: String,
    pub shipping_paths: Vec<String>,
    pub release_driver: ReleaseDriver,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Manifest {
    pub(crate) schema_version: u32,
    pub(crate) components: Vec<Component>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Component {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) tag_prefix: String,
    pub(crate) release_please_path: String,
    pub(crate) release_workflow: String,
    pub(crate) shipping_paths: Vec<String>,
    pub(crate) version_source: VersionFile,
    pub(crate) version_files: Vec<VersionFile>,
    /// The system that owns this component's normal version, tag, GitHub
    /// Release, and artifact-dispatch lifecycle. Every component must declare
    /// this explicitly so an omitted owner fails closed. `cli` uses
    /// `axon-native` because release-please's Cargo workspace plugin cannot
    /// handle `version.workspace = true` (googleapis/release-please#2111).
    /// Axon's xtask + auto-tag pipeline owns that component end to end instead.
    pub(crate) release_driver: ReleaseDriver,
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct VersionFile {
    pub(crate) kind: VersionKind,
    pub(crate) path: String,
    pub(crate) package: Option<String>,
    pub(crate) json_pointer: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VersionKind {
    CargoPackage,
    CargoLockPackage,
    ReadmeVersionLine,
    ChangelogHeading,
    JsonVersion,
    JsonNoVersion,
    NpmPackageLock,
    GradleVersionName,
    GradleVersionCode,
}
