//! Adapter-owned acquisition DTO aliases.

use axon_api::source::*;

pub type AcquisitionManifest = SourceManifest;
pub type AcquiredItem = AcquiredSourceItem;
pub type FetchStatus = LifecycleStatus;
