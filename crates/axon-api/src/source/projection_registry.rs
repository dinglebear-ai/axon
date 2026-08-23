use std::collections::BTreeSet;

use super::{ProjectionOperation, ProjectionResult, projection_error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionOperationSpec {
    pub operation: ProjectionOperation,
    pub cli_name: &'static str,
    pub mcp_name: &'static str,
    pub rest_path: &'static str,
    pub auth_scope: &'static str,
    pub mutating: bool,
    pub supports_batch: bool,
    pub supports_idempotency: bool,
    pub request_schema: &'static str,
    pub result_schema: &'static str,
}

pub const PROJECTION_OPERATIONS: &[ProjectionOperationSpec] = &[
    source_spec(ProjectionOperation::Scrape, "scrape", "ScrapeRequest"),
    source_spec(ProjectionOperation::Crawl, "crawl", "CrawlRequest"),
    source_spec(ProjectionOperation::Embed, "embed", "EmbedRequest"),
    source_spec(ProjectionOperation::Ingest, "ingest", "IngestRequest"),
    ProjectionOperationSpec {
        operation: ProjectionOperation::CodeSearch,
        cli_name: "code-search",
        mcp_name: "code_search",
        rest_path: "/v1/code-search",
        auth_scope: "axon:read",
        mutating: false,
        supports_batch: true,
        supports_idempotency: false,
        request_schema: "CodeSearchRequest",
        result_schema: "CodeSearchResult",
    },
];

const fn source_spec(
    operation: ProjectionOperation,
    name: &'static str,
    request_schema: &'static str,
) -> ProjectionOperationSpec {
    ProjectionOperationSpec {
        operation,
        cli_name: name,
        mcp_name: name,
        rest_path: match operation {
            ProjectionOperation::Scrape => "/v1/scrape",
            ProjectionOperation::Crawl => "/v1/crawl",
            ProjectionOperation::Embed => "/v1/embed",
            ProjectionOperation::Ingest => "/v1/ingest",
            ProjectionOperation::CodeSearch => "/v1/code-search",
        },
        auth_scope: "axon:write",
        mutating: true,
        supports_batch: true,
        supports_idempotency: true,
        request_schema,
        result_schema: "SourceResult",
    }
}

pub fn validate_projection_registry(specs: &[ProjectionOperationSpec]) -> ProjectionResult<()> {
    if specs.is_empty() {
        return Err(Box::new(projection_error(
            "projection.registry_empty",
            "projection registry must not be empty",
        )));
    }

    let mut cli_names = BTreeSet::new();
    let mut mcp_names = BTreeSet::new();
    let mut rest_paths = BTreeSet::new();
    for spec in specs {
        if !cli_names.insert(spec.cli_name)
            || !mcp_names.insert(spec.mcp_name)
            || !rest_paths.insert(spec.rest_path)
        {
            return Err(Box::new(projection_error(
                "projection.registry_duplicate",
                "projection registry transport names must be unique",
            )));
        }
    }
    Ok(())
}
