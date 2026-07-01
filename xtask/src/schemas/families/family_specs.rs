use super::{ExtraJsonSpec, FamilySpec, SchemaFamily};

pub(super) fn spec_for(family: SchemaFamily) -> FamilySpec {
    match family {
        SchemaFamily::Cli => cli_spec(),
        SchemaFamily::Openapi => openapi_spec(),
        SchemaFamily::Mcp => mcp_spec(),
        SchemaFamily::Config => config_spec(),
        SchemaFamily::Events => events_spec(),
        SchemaFamily::Database => database_spec(),
        SchemaFamily::Graph => graph_spec(),
        SchemaFamily::VectorPayload => vector_payload_spec(),
        SchemaFamily::Providers => providers_spec(),
        SchemaFamily::Api | SchemaFamily::Errors => unreachable!("real generator"),
    }
}

fn cli_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Cli,
        title: "AxonCliSchema",
        owner_crates: &["axon-cli", "axon-api"],
        source_paths: &[
            "crates/axon-cli/src",
            "docs/pipeline-unification/surfaces/command-contract.md",
        ],
        json_path: "docs/reference/cli/commands.json",
        extra_json: None,
        markdown_path: "docs/reference/cli/commands.md",
        extra_markdown_path: Some("docs/reference/cli/axon-help.md"),
    }
}

fn openapi_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Openapi,
        title: "AxonOpenApiSchema",
        owner_crates: &["axon-web", "axon-api"],
        source_paths: &[
            "crates/axon-web/src",
            "docs/pipeline-unification/surfaces/rest-contract.md",
        ],
        json_path: "docs/reference/rest/openapi.json",
        extra_json: None,
        markdown_path: "docs/reference/rest/openapi.md",
        extra_markdown_path: Some("docs/reference/rest/schemas.md"),
    }
}

fn mcp_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Mcp,
        title: "AxonMcpToolSchema",
        owner_crates: &["axon-mcp", "axon-api"],
        source_paths: &[
            "crates/axon-mcp/src",
            "docs/pipeline-unification/surfaces/tool-contract.md",
        ],
        json_path: "docs/reference/mcp/tool-schema.json",
        extra_json: Some(ExtraJsonSpec {
            path: "crates/axon-mcp/tests/golden/tool-schema.json",
            title: "AxonMcpToolSchema",
            id: "https://axon.local/schemas/mcp/tool.schema.json",
        }),
        markdown_path: "docs/reference/mcp/pipeline-tool-schema.md",
        extra_markdown_path: None,
    }
}

fn config_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Config,
        title: "AxonConfigSchema",
        owner_crates: &["axon-core"],
        source_paths: &[
            "crates/axon-core/src/config",
            "docs/pipeline-unification/configuration/config-contract.md",
        ],
        json_path: "docs/reference/config/config.schema.json",
        extra_json: Some(ExtraJsonSpec {
            path: "docs/reference/config/env.schema.json",
            title: "AxonEnvSchema",
            id: "https://axon.local/schemas/config/env.schema.json",
        }),
        markdown_path: "docs/reference/config/config-toml.md",
        extra_markdown_path: Some("docs/reference/config/env.md"),
    }
}

fn events_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Events,
        title: "AxonEventSchema",
        owner_crates: &["axon-observe"],
        source_paths: &[
            "crates/axon-observe/src",
            "docs/pipeline-unification/runtime/observability-contract.md",
        ],
        json_path: "docs/reference/runtime/events.schema.json",
        extra_json: None,
        markdown_path: "docs/reference/runtime/events.md",
        extra_markdown_path: None,
    }
}

fn database_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Database,
        title: "AxonDatabaseSchema",
        owner_crates: &["axon-jobs", "axon-ledger"],
        source_paths: &[
            "crates/axon-jobs/src/migrations",
            "docs/pipeline-unification/runtime/schema-contract.md",
        ],
        json_path: "docs/reference/runtime/database-schema.json",
        extra_json: None,
        markdown_path: "docs/reference/runtime/database-schema.md",
        extra_markdown_path: None,
    }
}

fn graph_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Graph,
        title: "AxonGraphSchema",
        owner_crates: &["axon-graph", "axon-parse"],
        source_paths: &[
            "crates/axon-graph/src/lib.rs",
            "docs/pipeline-unification/sources/source-graph.md",
        ],
        json_path: "docs/reference/sources/graph.schema.json",
        extra_json: None,
        markdown_path: "docs/reference/sources/graph.md",
        extra_markdown_path: None,
    }
}

fn vector_payload_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::VectorPayload,
        title: "AxonVectorPayloadSchema",
        owner_crates: &["axon-vectors", "axon-api"],
        source_paths: &[
            "crates/axon-vectors/src/lib.rs",
            "docs/pipeline-unification/schemas/vector-payload-schema.md",
        ],
        json_path: "docs/reference/sources/vector-payload.schema.json",
        extra_json: None,
        markdown_path: "docs/reference/sources/vector-payload.md",
        extra_markdown_path: None,
    }
}

fn providers_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Providers,
        title: "AxonProviderCapabilitySchema",
        owner_crates: &["axon-api", "axon-embedding", "axon-llm"],
        source_paths: &[
            "crates/axon-api/src/source/capability.rs",
            "docs/pipeline-unification/runtime/provider-contract.md",
        ],
        json_path: "docs/reference/runtime/provider-capabilities.schema.json",
        extra_json: None,
        markdown_path: "docs/reference/runtime/provider-capabilities.md",
        extra_markdown_path: None,
    }
}
