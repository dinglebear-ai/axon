//! CLI registry data: watch/monitor/map/endpoints/extract/search/research/brand/debug/diff/doctor/query/retrieve/ask/summarize/evaluate/train/suggest command families.
//! Split out of `cli_registry.rs` to stay under the repo's 500-line file cap; see that file for the shared `CliRegistryCommand` type and module docs.
//! Further split into per-family functions to stay under the 120-line function cap.
use super::{CliRegistryCommand, c};

pub(super) fn commands() -> Vec<CliRegistryCommand> {
    let mut commands = commands_watch_monitor();
    commands.extend(commands_map_endpoints_extract());
    commands.extend(commands_search_brand_debug_doctor());
    commands.extend(commands_query_ask_train());
    commands
}

fn commands_watch_monitor() -> Vec<CliRegistryCommand> {
    vec![
        // watch
        c(
            &["watch", "create"],
            "Create a recurring source watch",
            None,
            true,
            false,
            "write",
        ),
        c(
            &["watch", "list"],
            "List source watches",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["watch", "get"],
            "Show one source watch",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["watch", "status"],
            "Show source watch status",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["watch", "update"],
            "Update a source watch",
            None,
            true,
            false,
            "write",
        ),
        c(
            &["watch", "exec"],
            "Run a source watch immediately",
            None,
            true,
            true,
            "write",
        ),
        c(
            &["watch", "pause"],
            "Pause a source watch",
            None,
            true,
            false,
            "write",
        ),
        c(
            &["watch", "resume"],
            "Resume a paused source watch",
            None,
            true,
            false,
            "write",
        ),
        c(
            &["watch", "delete"],
            "Delete a source watch",
            None,
            true,
            false,
            "write",
        ),
        c(
            &["watch", "history"],
            "Show watch run history",
            None,
            false,
            false,
            "read",
        ),
        // monitor
        c(
            &["monitor", "jobs"],
            "Stream source and extract lifecycle events",
            None,
            false,
            false,
            "read",
        ),
    ]
}

fn commands_map_endpoints_extract() -> Vec<CliRegistryCommand> {
    vec![
        c(
            &["map"],
            "Discover all URLs on a site without scraping",
            Some("MapRequest"),
            false,
            false,
            "read",
        ),
        c(
            &["endpoints"],
            "Discover API endpoints from page HTML/JS bundles",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["extract"],
            "LLM-powered structured data extraction from URLs",
            Some("ExtractRequest"),
            true,
            true,
            "write",
        ),
        c(
            &["extract", "status"],
            "Show an extract job's status",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["extract", "cancel"],
            "Cancel a running extract job",
            None,
            true,
            false,
            "write",
        ),
        c(
            &["extract", "errors"],
            "Show an extract job's errors",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["extract", "list"],
            "List extract jobs",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["extract", "cleanup"],
            "Remove old terminal extract jobs",
            None,
            true,
            false,
            "write",
        ),
        c(
            &["extract", "clear"],
            "Clear all extract job rows",
            None,
            true,
            false,
            "write",
        ),
        c(
            &["extract", "worker"],
            "Run an extract worker inline",
            None,
            true,
            true,
            "admin",
        ),
        c(
            &["extract", "recover"],
            "Reclaim stale/interrupted extract jobs",
            None,
            true,
            false,
            "admin",
        ),
    ]
}

fn commands_search_brand_debug_doctor() -> Vec<CliRegistryCommand> {
    vec![
        // search / research
        c(
            &["search"],
            "Web search via SearXNG/Tavily, auto-queues Source jobs for results",
            Some("SearchRequest"),
            true,
            false,
            "read",
        ),
        c(
            &["research"],
            "Web research via SearXNG/Tavily with LLM synthesis and auto-indexing",
            Some("ResearchRequest"),
            true,
            false,
            "read",
        ),
        c(
            &["scrape"],
            "Fetch, normalize, and index exactly one web page through SourceRequest",
            Some("SourceRequest"),
            true,
            false,
            "write",
        ),
        // brand / debug / diff / doctor
        c(
            &["brand"],
            "Analyze a URL's brand identity: colors, fonts, logos, favicon",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["debug"],
            "Run doctor diagnostics plus LLM-assisted troubleshooting",
            None,
            false,
            false,
            "admin",
        ),
        c(
            &["diff"],
            "Diff two URLs — show what changed between them",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["doctor"],
            "Check connectivity to all required services",
            Some("DoctorRequest"),
            false,
            false,
            "admin",
        ),
        c(
            &["doctor", "diagnose"],
            "Print doctor output plus LLM diagnosis when configured",
            None,
            false,
            false,
            "admin",
        ),
    ]
}

fn commands_query_ask_train() -> Vec<CliRegistryCommand> {
    vec![
        // query / retrieve / ask / summarize / evaluate / train / suggest
        c(
            &["query"],
            "Semantic vector search over the Qdrant index",
            Some("VectorSearchRequest"),
            false,
            false,
            "read",
        ),
        c(
            &["retrieve"],
            "Fetch stored document chunks from Qdrant by URL",
            Some("RetrieveRequest"),
            false,
            false,
            "read",
        ),
        c(
            &["ask"],
            "RAG: retrieve relevant context, then answer with LLM",
            Some("AskRequest"),
            false,
            false,
            "read",
        ),
        c(
            &["summarize"],
            "Scrape one or more URLs and summarize them with the configured LLM",
            Some("SummarizeRequest"),
            false,
            false,
            "read",
        ),
        c(
            &["evaluate"],
            "RAG vs baseline with independent LLM judge scoring",
            Some("EvaluateRequest"),
            false,
            false,
            "read",
        ),
        c(
            &["train"],
            "Collect human preference votes for retrieved RAG candidates",
            None,
            true,
            false,
            "write",
        ),
        c(
            &["suggest"],
            "Suggest new documentation URLs to index",
            Some("SuggestRequest"),
            false,
            false,
            "read",
        ),
    ]
}
