//! Service result contracts grouped by domain.
//!
//! Public types are re-exported here for backwards-compatible imports through
//! `crate::types::*`, while the concrete definitions live in focused
//! modules under `types/service/`.

mod brand;
mod content;
mod diff;
mod lifecycle;
mod options;
mod query;
mod system;

pub use brand::*;
pub use content::*;
pub use diff::*;
pub use lifecycle::*;
pub use options::*;
pub use query::*;
pub use system::*;
