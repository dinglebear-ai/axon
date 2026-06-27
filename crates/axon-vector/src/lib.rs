pub mod cache;
pub mod ops;

/// Domain service entry for the `purge` operation: delete indexed points by URL
/// (or seed-URL/origin prefix). Returns the transport-neutral
/// [`axon_api::purge::PurgeResult`]. This is the public, typed boundary —
/// callers go through this (or the `axon-services` re-export), never reach into
/// `ops::qdrant` internals.
pub use ops::qdrant::qdrant_delete_by_url as purge;
