//! Service type definitions, split into two sub-modules:
//!
//! - [`service`] — Generic service result types (query, scrape, system, etc.)
//!
//! All public types are re-exported here for backward compatibility so that
//! `use crate::services::types::SomeType` continues to work unchanged.

pub mod client_server;
mod contracts;
mod endpoints;
mod service;

pub use client_server::*;
pub use contracts::*;
pub use endpoints::*;
pub use service::*;
