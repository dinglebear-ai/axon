//! Compatibility shim: ServiceEvent + progress-channel helpers now live in
//! `axon_core::events`. Re-exported so existing `crate::events::*`
//! call sites resolve unchanged.
pub use axon_core::events::*;
