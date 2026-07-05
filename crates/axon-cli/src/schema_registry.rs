//! CLI command registry used by schema-contract generation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliCommandSpec {
    pub name: &'static str,
    pub maps_to_dto: Option<&'static str>,
    pub mutates: bool,
    pub async_job: bool,
    pub required_scope: &'static str,
}

pub fn command_registry() -> &'static [CliCommandSpec] {
    &[
        CliCommandSpec {
            name: "ask",
            maps_to_dto: Some("AskRequest"),
            mutates: false,
            async_job: false,
            required_scope: "read",
        },
        CliCommandSpec {
            name: "query",
            maps_to_dto: Some("VectorSearchRequest"),
            mutates: false,
            async_job: false,
            required_scope: "read",
        },
        CliCommandSpec {
            name: "retrieve",
            maps_to_dto: Some("RetrieveRequest"),
            mutates: false,
            async_job: false,
            required_scope: "read",
        },
        CliCommandSpec {
            name: "search",
            maps_to_dto: Some("SearchRequest"),
            mutates: false,
            async_job: false,
            required_scope: "read",
        },
        CliCommandSpec {
            name: "research",
            maps_to_dto: Some("ResearchRequest"),
            mutates: false,
            async_job: false,
            required_scope: "read",
        },
        CliCommandSpec {
            name: "summarize",
            maps_to_dto: Some("SummarizeRequest"),
            mutates: false,
            async_job: false,
            required_scope: "read",
        },
        CliCommandSpec {
            name: "evaluate",
            maps_to_dto: Some("EvaluateRequest"),
            mutates: false,
            async_job: false,
            required_scope: "read",
        },
        CliCommandSpec {
            name: "suggest",
            maps_to_dto: Some("SuggestRequest"),
            mutates: false,
            async_job: false,
            required_scope: "read",
        },
        CliCommandSpec {
            name: "map",
            maps_to_dto: Some("MapRequest"),
            mutates: false,
            async_job: false,
            required_scope: "read",
        },
        CliCommandSpec {
            name: "extract",
            maps_to_dto: Some("ExtractRequest"),
            mutates: true,
            async_job: true,
            required_scope: "write",
        },
        CliCommandSpec {
            name: "sources",
            maps_to_dto: Some("SourcesRequest"),
            mutates: false,
            async_job: false,
            required_scope: "read",
        },
        CliCommandSpec {
            name: "domains",
            maps_to_dto: Some("DomainsRequest"),
            mutates: false,
            async_job: false,
            required_scope: "read",
        },
        CliCommandSpec {
            name: "stats",
            maps_to_dto: Some("StatsRequest"),
            mutates: false,
            async_job: false,
            required_scope: "read",
        },
        CliCommandSpec {
            name: "doctor",
            maps_to_dto: Some("DoctorRequest"),
            mutates: false,
            async_job: false,
            required_scope: "admin",
        },
        CliCommandSpec {
            name: "config",
            maps_to_dto: Some("ConfigProjectionRequest"),
            mutates: true,
            async_job: false,
            required_scope: "admin",
        },
    ]
}

pub fn removed_commands() -> &'static [&'static str] {
    &[
        "embed",
        "ingest",
        "scrape",
        "crawl",
        "code-search",
        "code-search-watch",
        "purge",
        "dedupe",
        "refresh",
        "fresh",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_registry_contains_canonical_commands_and_removed_metadata() {
        let names = command_registry()
            .iter()
            .map(|command| command.name)
            .collect::<std::collections::BTreeSet<_>>();
        assert!(names.contains("map"));
        assert!(names.contains("extract"));
        assert!(!names.contains("scrape"));
        assert!(removed_commands().contains(&"scrape"));
    }
}
