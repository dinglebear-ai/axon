//! Marker module for the target `axon-memory::graph` boundary.

pub const MODULE_NAME: &str = "graph";
pub const MEMORY_GRAPH_REQUIRED_FACT: &str = "memory_document";
pub const MEMORY_GRAPH_OPTIONAL_FACTS: &[&str] = &["memory_link", "supersedes"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryGraphCandidate {
    pub memory_id: String,
    pub fact_kind: &'static str,
}

pub fn memory_graph_candidates(memory_id: impl Into<String>) -> Vec<MemoryGraphCandidate> {
    vec![MemoryGraphCandidate {
        memory_id: memory_id.into(),
        fact_kind: MEMORY_GRAPH_REQUIRED_FACT,
    }]
}
