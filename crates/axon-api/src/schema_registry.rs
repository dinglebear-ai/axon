//! Transport DTO and enum registry used by schema-contract generation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DtoSchemaSpec {
    pub name: &'static str,
    pub family: &'static str,
    pub transport_exposed: bool,
    pub store_exposed: bool,
}

pub fn dto_schema_registry() -> &'static [DtoSchemaSpec] {
    &[
        DtoSchemaSpec {
            name: "Envelope",
            family: "Envelope",
            transport_exposed: true,
            store_exposed: false,
        },
        DtoSchemaSpec {
            name: "SourceRecord",
            family: "Source",
            transport_exposed: true,
            store_exposed: true,
        },
        DtoSchemaSpec {
            name: "LedgerEntry",
            family: "Ledger",
            transport_exposed: true,
            store_exposed: true,
        },
        DtoSchemaSpec {
            name: "DocumentRecord",
            family: "Document",
            transport_exposed: true,
            store_exposed: true,
        },
        DtoSchemaSpec {
            name: "GraphNode",
            family: "Parse/Graph",
            transport_exposed: true,
            store_exposed: true,
        },
        DtoSchemaSpec {
            name: "VectorSearchRequest",
            family: "Embedding/Vector",
            transport_exposed: true,
            store_exposed: false,
        },
        DtoSchemaSpec {
            name: "RetrieveRequest",
            family: "Retrieval",
            transport_exposed: true,
            store_exposed: false,
        },
        DtoSchemaSpec {
            name: "AskRequest",
            family: "Discovery/Synthesis",
            transport_exposed: true,
            store_exposed: false,
        },
        DtoSchemaSpec {
            name: "JobEvent",
            family: "Runtime",
            transport_exposed: true,
            store_exposed: true,
        },
        DtoSchemaSpec {
            name: "ResetPlan",
            family: "Operations",
            transport_exposed: true,
            store_exposed: false,
        },
        DtoSchemaSpec {
            name: "ApiError",
            family: "Errors",
            transport_exposed: true,
            store_exposed: false,
        },
        DtoSchemaSpec {
            name: "MemoryRecord",
            family: "Memory",
            transport_exposed: true,
            store_exposed: true,
        },
        DtoSchemaSpec {
            name: "ConfigProjection",
            family: "Config/setup/serve/MCP/palette operational DTOs",
            transport_exposed: true,
            store_exposed: false,
        },
        DtoSchemaSpec {
            name: "ProviderCapability",
            family: "Provider capability DTOs",
            transport_exposed: true,
            store_exposed: true,
        },
    ]
}

pub fn enum_schema_registry() -> &'static [&'static str] {
    &[
        "SourceKind",
        "SourceLifecycleStatus",
        "PipelinePhase",
        "JobKind",
        "ErrorCode",
        "ErrorStage",
    ]
}

pub fn removed_dto_names() -> &'static [&'static str] {
    &[
        "EmbedRequest",
        "IngestRequest",
        "CrawlRequest",
        "ScrapeRequest",
        "CodeSearchRequest",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_registry_covers_required_contract_families() {
        let families = dto_schema_registry()
            .iter()
            .map(|dto| dto.family)
            .collect::<std::collections::BTreeSet<_>>();
        for required in [
            "Envelope",
            "Source",
            "Ledger",
            "Document",
            "Parse/Graph",
            "Embedding/Vector",
            "Retrieval",
            "Discovery/Synthesis",
            "Runtime",
            "Operations",
            "Errors",
            "Memory",
            "Config/setup/serve/MCP/palette operational DTOs",
            "Provider capability DTOs",
        ] {
            assert!(families.contains(required), "missing DTO family {required}");
        }
    }
}
