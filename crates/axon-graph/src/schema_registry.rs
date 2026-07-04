//! Source graph registry used by schema-contract generation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphKindSpec {
    pub kind: &'static str,
    pub requires_evidence: bool,
}

pub fn node_kind_registry() -> &'static [GraphKindSpec] {
    &[
        GraphKindSpec {
            kind: "document",
            requires_evidence: true,
        },
        GraphKindSpec {
            kind: "chunk",
            requires_evidence: true,
        },
        GraphKindSpec {
            kind: "entity",
            requires_evidence: true,
        },
    ]
}

pub fn edge_kind_registry() -> &'static [GraphKindSpec] {
    &[
        GraphKindSpec {
            kind: "contains",
            requires_evidence: true,
        },
        GraphKindSpec {
            kind: "mentions",
            requires_evidence: true,
        },
        GraphKindSpec {
            kind: "derived_from",
            requires_evidence: true,
        },
    ]
}
