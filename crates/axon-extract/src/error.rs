//! VerticalError — thin re-export of the a9l6 taxonomy variants that
//! vertical extractors can produce. Callers map these to
//! `ServiceTaxonomyError` for MCP/CLI surfaces.

pub use axon_core::error::ServiceTaxonomyError as VerticalError;
