//! Service type definitions, split into two sub-modules:
//!
//! - [`acp`] — ACP protocol types (bridge events, session setup, config options)
//! - [`service`] — Generic service result types (query, scrape, system, etc.)
//!
//! All public types are re-exported here for backward compatibility so that
//! `use crate::services::types::SomeType` continues to work unchanged.

mod acp;
mod contracts;
mod service;

// Re-export everything from both sub-modules at the `types` level.
pub use acp::*;
pub use contracts::*;
pub use service::*;
