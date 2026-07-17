//! Read/query primitives ported from legacy `axon-vector`'s Qdrant ops.
//!
//! These are inherent methods on [`super::QdrantVectorStore`] (not part of the
//! [`crate::store::VectorStore`] trait) because they operate on the raw
//! target vector-payload shape the source pipeline writes today. These methods
//! exist so `axon-services` (and ultimately the CLI/MCP/REST commands it backs)
//! can read Qdrant directly through `axon-vectors` instead of `axon-vector`, so
//! the legacy crate can eventually be deleted (axon #298).
//!
//! Every method here reuses [`super::http::QdrantHttp`] (via
//! `QdrantVectorStore::http()`), so retries, redaction, and error shape stay
//! identical to the rest of the crate's Qdrant transport.

mod domain;
mod facet;
mod retrieve;
mod scroll;

pub use retrieve::{QdrantRetrieveByUrlResult, QdrantUrlVariantError, render_full_doc_from_points};
pub use scroll::{QdrantScrolledPoint, ScrollPage};
