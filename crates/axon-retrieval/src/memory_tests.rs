use crate::memory::{MEMORY_SOURCE_KIND, matches_memory_source_kind, memory_retrieval_filter};

#[test]
fn memory_retrieval_filters_to_memory_source_kind() {
    let filter = memory_retrieval_filter();

    assert_eq!(filter.source_kind, MEMORY_SOURCE_KIND);
    assert!(matches_memory_source_kind("memory"));
    assert!(!matches_memory_source_kind("source:memory"));
    assert!(!matches_memory_source_kind("web"));
}
