//! Adapter-owned acquisition DTO aliases.

use axon_api::source::*;
use std::path::{Path, PathBuf};

pub type AcquisitionManifest = SourceManifest;
pub type AcquiredItem = AcquiredSourceItem;
pub type FetchStatus = LifecycleStatus;

/// Adapter-owned prepared input plus the routed plan that produced it.
/// `_temporary` keeps throwaway acquisitions (notably git clones) alive until
/// the service bridge has finished the complete source run.
#[derive(Debug)]
pub struct MaterializedSource {
    pub plan: SourcePlan,
    path: PathBuf,
    _temporary: Option<tempfile::TempDir>,
}

impl MaterializedSource {
    /// Construct a materialized source whose acquired state is held by the
    /// adapter rather than represented by a filesystem path.
    pub fn virtual_source(plan: SourcePlan) -> Self {
        Self {
            plan,
            path: PathBuf::new(),
            _temporary: None,
        }
    }

    pub fn persistent(plan: SourcePlan, path: PathBuf) -> Self {
        Self {
            plan,
            path,
            _temporary: None,
        }
    }

    pub fn temporary(plan: SourcePlan, temporary: tempfile::TempDir) -> Self {
        let path = temporary.path().to_path_buf();
        Self {
            plan,
            path,
            _temporary: Some(temporary),
        }
    }

    pub fn temporary_at(plan: SourcePlan, temporary: tempfile::TempDir, path: PathBuf) -> Self {
        debug_assert!(path.starts_with(temporary.path()));
        Self {
            plan,
            path,
            _temporary: Some(temporary),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub(crate) fn materialization_error(code: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::new(code, axon_error::ErrorStage::Fetching, message)
}
