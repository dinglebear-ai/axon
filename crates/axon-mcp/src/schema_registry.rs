//! MCP action registry used by schema-contract generation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McpActionSpec {
    pub action: &'static str,
    pub request_dto: &'static str,
    pub result_dto: &'static str,
    pub required_scope: &'static str,
    pub mutates: bool,
    pub async_job: bool,
}

pub fn action_registry() -> &'static [McpActionSpec] {
    &[
        McpActionSpec {
            action: "ask",
            request_dto: "AskRequest",
            result_dto: "AskResponse",
            required_scope: "read",
            mutates: false,
            async_job: false,
        },
        McpActionSpec {
            action: "query",
            request_dto: "VectorSearchRequest",
            result_dto: "VectorSearchResult",
            required_scope: "read",
            mutates: false,
            async_job: false,
        },
        McpActionSpec {
            action: "retrieve",
            request_dto: "RetrieveRequest",
            result_dto: "RetrieveResponse",
            required_scope: "read",
            mutates: false,
            async_job: false,
        },
        McpActionSpec {
            action: "search",
            request_dto: "SearchRequest",
            result_dto: "SearchResponse",
            required_scope: "read",
            mutates: false,
            async_job: false,
        },
        McpActionSpec {
            action: "research",
            request_dto: "ResearchRequest",
            result_dto: "ResearchResponse",
            required_scope: "read",
            mutates: false,
            async_job: false,
        },
        McpActionSpec {
            action: "map",
            request_dto: "MapRequest",
            result_dto: "MapResponse",
            required_scope: "read",
            mutates: false,
            async_job: false,
        },
        McpActionSpec {
            action: "extract",
            request_dto: "ExtractRequest",
            result_dto: "ExtractResponse",
            required_scope: "write",
            mutates: true,
            async_job: true,
        },
        McpActionSpec {
            action: "config",
            request_dto: "ConfigProjectionRequest",
            result_dto: "ConfigProjectionResponse",
            required_scope: "admin",
            mutates: true,
            async_job: false,
        },
        McpActionSpec {
            action: "resolve",
            request_dto: "ResolveRequest",
            result_dto: "RoutePlan",
            required_scope: "read",
            mutates: false,
            async_job: false,
        },
        McpActionSpec {
            action: "capabilities",
            request_dto: "CapabilitiesRequest",
            result_dto: "CapabilityDocument",
            required_scope: "read",
            mutates: false,
            async_job: false,
        },
        McpActionSpec {
            action: "providers",
            request_dto: "ProvidersRequest",
            result_dto: "ProviderSummary",
            required_scope: "read",
            mutates: false,
            async_job: false,
        },
    ]
}

pub fn removed_actions() -> &'static [&'static str] {
    &[
        "embed",
        "ingest",
        "scrape",
        "crawl",
        "code_search",
        "code_search_watch",
        "vertical_scrape",
        "purge",
        "dedupe",
        "sources",
        "domains",
        "stats",
        "elicit_demo",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_registry_contains_canonical_actions_and_removed_metadata() {
        let actions = action_registry()
            .iter()
            .map(|action| action.action)
            .collect::<std::collections::BTreeSet<_>>();
        assert!(actions.contains("map"));
        assert!(actions.contains("extract"));
        assert!(!actions.contains("crawl"));
        assert!(removed_actions().contains(&"crawl"));
    }
}
