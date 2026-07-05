use crate::memory::{MEMORY_VECTOR_NAMESPACE, matches_memory_namespace, memory_retrieval_filter};

#[test]
fn memory_retrieval_filters_to_memory_namespace() {
    let filter = memory_retrieval_filter();

    assert_eq!(filter.vector_namespace, MEMORY_VECTOR_NAMESPACE);
    assert!(matches_memory_namespace("memory"));
    assert!(!matches_memory_namespace("source:memory"));
    assert!(!matches_memory_namespace("source:web"));
}
