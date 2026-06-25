//! Compatibility shim: `JobStatus` now lives in `axon_api::job_status`.
//! Re-exported so existing `crate::status::JobStatus` call sites resolve
//! unchanged.
pub use axon_api::job_status::JobStatus;
