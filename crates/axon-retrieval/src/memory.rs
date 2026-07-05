//! Marker module for the target `axon-retrieval::memory` boundary.

pub const MODULE_NAME: &str = "memory";
pub const MEMORY_VECTOR_NAMESPACE: &str = "memory";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRetrievalFilter {
    pub vector_namespace: &'static str,
}

pub fn memory_retrieval_filter() -> MemoryRetrievalFilter {
    MemoryRetrievalFilter {
        vector_namespace: MEMORY_VECTOR_NAMESPACE,
    }
}

pub fn matches_memory_namespace(namespace: &str) -> bool {
    namespace == MEMORY_VECTOR_NAMESPACE
}
