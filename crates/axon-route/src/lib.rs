//! Source resolution and routing for the unified source pipeline.

pub mod alias;
pub mod authority;
pub mod canonical;
pub mod capability;
mod github;
pub mod local_path;
mod provider_host;
mod query;
pub mod resolver;
pub mod router;
pub mod scope;
pub mod source_id;
pub mod testing;

pub use alias::AliasRecord;
pub use authority::{AuthorityRecord, InMemoryAuthorityRegistry};
pub use axon_api::{ResolvedSource, RoutePlan, SourceId, SourceScope};
pub use capability::{AdapterDefinition, AdapterRegistry};
pub use resolver::SourceResolver;
pub use router::{RouteDecision, RouteSecurityPolicy, SourceRouter};

pub const CRATE_NAME: &str = "axon-route";
pub type AdapterMatch = AdapterDefinition;
pub type CanonicalUri = String;

#[cfg(test)]
#[path = "route_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "route_normalization_tests.rs"]
mod route_normalization_tests;

#[cfg(test)]
#[path = "route_validation_tests.rs"]
mod route_validation_tests;
