//! Marker module for the target `axon-retrieval::memory` boundary.

pub const MODULE_NAME: &str = "memory";
pub const MEMORY_SOURCE_KIND: &str = "memory";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRetrievalFilter {
    pub source_kind: &'static str,
}

pub fn memory_retrieval_filter() -> MemoryRetrievalFilter {
    MemoryRetrievalFilter {
        source_kind: MEMORY_SOURCE_KIND,
    }
}

pub fn matches_memory_source_kind(source_kind: &str) -> bool {
    source_kind == MEMORY_SOURCE_KIND
}
