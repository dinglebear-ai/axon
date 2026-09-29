//! Runtime action policy, typed schema callbacks and descriptive metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActionScope {
    Read,
    Write,
    /// Destructive/admin-gated action. Per the auth contract, `axon:write`
    /// does NOT imply `axon:admin` — the caller must hold the fine-grained
    /// scope explicitly.
    Admin,
    InfoOnly,
}

impl ActionScope {
    pub(crate) fn as_scope(self, _subaction: &str) -> Option<&'static str> {
        match self {
            Self::Read => Some("axon:read"),
            Self::Write => Some("axon:write"),
            Self::Admin => Some("axon:admin"),
            Self::InfoOnly => None,
        }
    }

    pub(crate) fn as_label(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Admin => "admin",
            Self::InfoOnly => "info",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct McpSafetyHints {
    pub read_only: bool,
    pub destructive: bool,
    pub idempotent: Option<bool>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct McpActionSpec {
    pub name: &'static str,
    pub scope: ActionScope,
    pub description: &'static str,
    pub cost: &'static str,
    pub request_dto: &'static str,
    pub request_schema: fn() -> serde_json::Value,
    pub async_job: bool,
}

fn request_schema<T: schemars::JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(T)).expect("serialize MCP action request schema")
}

impl McpActionSpec {
    /// Conservative MCP safety annotations for the whole canonical action.
    /// These are deliberately independent from authorization scope because
    /// some read-gated actions can enqueue indexing work and mixed subaction
    /// families contain both reads and mutations.
    pub(crate) fn safety_hints(self) -> McpSafetyHints {
        let read_only = matches!(
            self.name,
            "code_search"
                | "help"
                | "status"
                | "doctor"
                | "query"
                | "retrieve"
                | "resolve"
                | "capabilities"
                | "providers"
                | "map"
                | "collections"
                | "evaluate"
                | "suggest"
                | "summarize"
                | "graph"
                | "artifacts"
                | "chat"
        );
        let destructive = matches!(
            self.name,
            "codex" | "jobs" | "prune" | "reset" | "memory" | "watch" | "uploads"
        );

        McpSafetyHints {
            read_only,
            destructive,
            idempotent: if read_only { Some(true) } else { None },
        }
    }
}

pub(crate) const MCP_ACTION_SPECS: &[McpActionSpec] = &[
    McpActionSpec {
        name: "codex",
        async_job: false,
        request_dto: "CodexRequest",
        request_schema: request_schema::<crate::schema::CodexRequest>,
        scope: ActionScope::Admin,
        description: "Inspect and operate the approval-gated Codex app-server control plane",
        cost: "write",
    },
    McpActionSpec {
        name: "scrape",
        async_job: true,
        request_dto: "ScrapeRequest",
        request_schema: request_schema::<axon_api::source::ScrapeRequest>,
        scope: ActionScope::Write,
        description: "Project one or more page acquisitions onto the canonical source pipeline",
        cost: "write",
    },
    McpActionSpec {
        name: "crawl",
        async_job: true,
        request_dto: "CrawlRequest",
        request_schema: request_schema::<axon_api::source::CrawlRequest>,
        scope: ActionScope::Write,
        description: "Project one or more site crawls onto the canonical source pipeline",
        cost: "write",
    },
    McpActionSpec {
        name: "embed",
        async_job: true,
        request_dto: "EmbedRequest",
        request_schema: request_schema::<axon_api::source::EmbedRequest>,
        scope: ActionScope::Write,
        description: "Project one or more embedding requests onto the canonical source pipeline",
        cost: "write",
    },
    McpActionSpec {
        name: "ingest",
        async_job: true,
        request_dto: "IngestRequest",
        request_schema: request_schema::<axon_api::source::IngestRequest>,
        scope: ActionScope::Write,
        description: "Project one or more ingestion requests onto the canonical source pipeline",
        cost: "write",
    },
    McpActionSpec {
        name: "code_search",
        async_job: false,
        request_dto: "CodeSearchRequest",
        request_schema: request_schema::<axon_api::source::CodeSearchRequest>,
        scope: ActionScope::Read,
        description: "Search committed code vectors without refreshing the index",
        cost: "cheap",
    },
    McpActionSpec {
        name: "help",
        async_job: false,
        request_dto: "HelpRequest",
        request_schema: request_schema::<crate::schema::HelpRequest>,
        scope: ActionScope::InfoOnly,
        description: "List actions, subactions, defaults, and schema resource links",
        cost: "cheap",
    },
    McpActionSpec {
        name: "status",
        async_job: false,
        request_dto: "StatusRequest",
        request_schema: request_schema::<crate::schema::StatusRequest>,
        scope: ActionScope::Read,
        description: "Show job queue, worker, and service status",
        cost: "cheap",
    },
    McpActionSpec {
        name: "jobs",
        async_job: false,
        request_dto: "JobsRequest",
        request_schema: request_schema::<crate::schema::JobsRequest>,
        scope: ActionScope::Write,
        description: "List, inspect, page events, cancel, retry, recover, cleanup, or clear unified durable jobs",
        cost: "write",
    },
    McpActionSpec {
        name: "doctor",
        async_job: false,
        request_dto: "DoctorRequest",
        request_schema: request_schema::<crate::schema::DoctorRequest>,
        scope: ActionScope::Read,
        description: "Diagnose Axon service connectivity",
        cost: "cheap",
    },
    McpActionSpec {
        name: "source",
        async_job: true,
        request_dto: "SourceRequest",
        request_schema: request_schema::<crate::schema::SourceRequest>,
        scope: ActionScope::Write,
        description: "Acquire and index one source (local path, git/web/feed/youtube/reddit/session/registry target) through the unified pipeline",
        cost: "write",
    },
    McpActionSpec {
        name: "query",
        async_job: false,
        request_dto: "QueryRequest",
        request_schema: request_schema::<crate::schema::QueryRequest>,
        scope: ActionScope::Read,
        description: "Run semantic vector search over indexed content",
        cost: "cheap",
    },
    McpActionSpec {
        name: "retrieve",
        async_job: false,
        request_dto: "RetrieveRequest",
        request_schema: request_schema::<crate::schema::RetrieveRequest>,
        scope: ActionScope::Read,
        description: "Fetch stored document chunks by URL",
        cost: "cheap",
    },
    // resolve/capabilities/providers (contract's `resolve`/`capabilities`/
    // `providers` actions, WS-G #298): read-only discovery surfaces backed by
    // real data — `resolve` calls `axon_services::source::routing::
    // resolve_source_route`, `providers` reshapes `system::doctor`'s per-
    // service payload (mirroring the REST `/v1/providers` resource-tier
    // routes), and `capabilities` reports the live `MCP_ACTION_SPECS`
    // registry plus provider health. None of these mutate state.
    McpActionSpec {
        name: "resolve",
        async_job: false,
        request_dto: "ResolveRequest",
        request_schema: request_schema::<crate::schema::ResolveRequest>,
        scope: ActionScope::Read,
        description: "Resolve source identity and adapter route without acquiring content",
        cost: "cheap",
    },
    McpActionSpec {
        name: "capabilities",
        async_job: false,
        request_dto: "CapabilitiesRequest",
        request_schema: request_schema::<crate::schema::CapabilitiesRequest>,
        scope: ActionScope::Read,
        description: "Machine-readable server capability document: actions, scopes, providers",
        cost: "cheap",
    },
    McpActionSpec {
        name: "providers",
        async_job: false,
        request_dto: "ProvidersRequest",
        request_schema: request_schema::<crate::schema::ProvidersRequest>,
        scope: ActionScope::Read,
        description: "List or inspect provider capability/health (list|get subactions)",
        cost: "cheap",
    },
    McpActionSpec {
        name: "search",
        async_job: false,
        request_dto: "SearchRequest",
        request_schema: request_schema::<crate::schema::SearchRequest>,
        scope: ActionScope::Read,
        description: "Run SearXNG/Tavily web search and optionally queue source auto-index jobs for results",
        cost: "moderate",
    },
    McpActionSpec {
        name: "map",
        async_job: false,
        request_dto: "MapRequest",
        request_schema: request_schema::<crate::schema::MapRequest>,
        scope: ActionScope::Read,
        description: "Discover URLs for a site without scraping page content",
        cost: "moderate",
    },
    McpActionSpec {
        name: "prune",
        async_job: false,
        request_dto: "PruneMcpRequest",
        request_schema: request_schema::<crate::schema::PruneMcpRequest>,
        scope: ActionScope::Admin,
        description: "Plan or execute source, generation, or collection cleanup behind axon-prune",
        cost: "write",
    },
    McpActionSpec {
        name: "collections",
        async_job: false,
        request_dto: "CollectionsMcpRequest",
        request_schema: request_schema::<crate::server::system_requests::CollectionsMcpRequest>,
        scope: ActionScope::Read,
        description: "List or inspect configured vector collections",
        cost: "cheap",
    },
    McpActionSpec {
        name: "reset",
        async_job: false,
        request_dto: "ResetMcpRequest",
        request_schema: request_schema::<crate::server::system_requests::ResetMcpRequest>,
        scope: ActionScope::Admin,
        description: "Plan or execute an explicit clean-slate store reset",
        cost: "write",
    },
    // U2-20/C6-20: ask/evaluate/suggest/research/summarize default to
    // `axon:read` — they're query-shaped surfaces, even though research (and
    // occasionally ask/summarize) may enqueue a background source/index job as
    // a side effect. No `mutates_if`/conditional-upgrade metadata exists yet
    // (tracked as a follow-up); until it lands these stay read-gated rather
    // than write-gated, matching the contract's stated default.
    McpActionSpec {
        name: "ask",
        async_job: false,
        request_dto: "AskRequest",
        request_schema: request_schema::<crate::schema::AskRequest>,
        scope: ActionScope::Read,
        description: "Answer a question with RAG over indexed content",
        cost: "moderate",
    },
    McpActionSpec {
        name: "evaluate",
        async_job: false,
        request_dto: "EvaluateRequest",
        request_schema: request_schema::<crate::schema::EvaluateRequest>,
        scope: ActionScope::Read,
        description: "Evaluate RAG quality against a baseline and judge diagnostics",
        cost: "expensive",
    },
    McpActionSpec {
        name: "suggest",
        async_job: false,
        request_dto: "SuggestRequest",
        request_schema: request_schema::<crate::schema::SuggestRequest>,
        scope: ActionScope::Read,
        description: "Suggest new documentation URLs to index",
        cost: "moderate",
    },
    McpActionSpec {
        name: "research",
        async_job: false,
        request_dto: "ResearchRequest",
        request_schema: request_schema::<crate::schema::ResearchRequest>,
        scope: ActionScope::Read,
        description: "Run SearXNG/Tavily research with synthesis and auto-indexing",
        cost: "expensive",
    },
    McpActionSpec {
        name: "screenshot",
        async_job: false,
        request_dto: "ScreenshotRequest",
        request_schema: request_schema::<crate::schema::ScreenshotRequest>,
        scope: ActionScope::Write,
        description: "Capture a full-page screenshot through headless Chrome",
        cost: "moderate",
    },
    McpActionSpec {
        name: "brand",
        async_job: false,
        request_dto: "BrandRequest",
        request_schema: request_schema::<crate::schema::BrandRequest>,
        scope: ActionScope::Write,
        description: "Extract brand identity metadata from a URL",
        cost: "write",
    },
    McpActionSpec {
        name: "diff",
        async_job: false,
        request_dto: "DiffRequest",
        request_schema: request_schema::<crate::schema::DiffRequest>,
        scope: ActionScope::Write,
        description: "Compare two URLs for content, metadata, and link changes",
        cost: "write",
    },
    McpActionSpec {
        name: "extract",
        async_job: true,
        request_dto: "ExtractRequest",
        request_schema: request_schema::<crate::schema::ExtractRequest>,
        scope: ActionScope::Write,
        description: "Start async structured extraction jobs; use action=jobs for lifecycle",
        cost: "write",
    },
    McpActionSpec {
        name: "memory",
        async_job: false,
        request_dto: "MemoryRequest",
        request_schema: request_schema::<crate::schema::MemoryRequest>,
        scope: ActionScope::Write,
        description: "Remember, search, and show persistent agent memory",
        cost: "write",
    },
    McpActionSpec {
        name: "summarize",
        async_job: false,
        request_dto: "SummarizeRequest",
        request_schema: request_schema::<crate::schema::SummarizeRequest>,
        scope: ActionScope::Read,
        description: "Fetch URL context and summarize it with the configured LLM",
        cost: "write",
    },
    McpActionSpec {
        name: "endpoints",
        async_job: false,
        request_dto: "EndpointsRequest",
        request_schema: request_schema::<crate::schema::EndpointsRequest>,
        scope: ActionScope::Write,
        description: "Discover and optionally verify static site endpoints",
        cost: "write",
    },
    // `watch` (issue #298 WS-B): source-request-backed watch subactions mirror
    // the REST `/v1/watches` surface. Per-subaction scope is enforced in
    // `required_scope_for` below (`list`/`get`/`history` read; mutating
    // lifecycle operations write).
    McpActionSpec {
        name: "watch",
        async_job: false,
        request_dto: "WatchMcpRequest",
        request_schema: request_schema::<crate::server::system_requests::WatchMcpRequest>,
        scope: ActionScope::Write,
        description: "Create, list, inspect, update, pause, resume, or delete source-request-backed watches",
        cost: "write",
    },
    // `graph` (issue #298 GQ): read-only SourceGraph query surface mirroring
    // the REST `/v1/graph/*` routes. All subactions (`kinds`/`resolve`/
    // `query`/`node`/`edge`/`source`) are pure reads — graph writes stay
    // parser/source-job owned, never caller-provided through this action.
    McpActionSpec {
        name: "graph",
        async_job: false,
        request_dto: "GraphRequest",
        request_schema: request_schema::<crate::schema::GraphRequest>,
        scope: ActionScope::Read,
        description: "Query the read-only SourceGraph: kinds, resolve, query, node, edge, source subgraph",
        cost: "cheap",
    },
    McpActionSpec {
        name: "uploads",
        async_job: false,
        request_dto: "UploadsMcpRequest",
        request_schema: request_schema::<crate::server::system_requests::UploadsMcpRequest>,
        scope: ActionScope::Write,
        description: "Stage, inspect, complete, list, or abort durable uploads",
        cost: "write",
    },
    McpActionSpec {
        name: "artifacts",
        async_job: false,
        request_dto: "ArtifactsMcpRequest",
        request_schema: request_schema::<crate::server::system_requests::ArtifactsMcpRequest>,
        scope: ActionScope::Read,
        description: "List, inspect, or read artifacts by opaque artifact id",
        cost: "cheap",
    },
    McpActionSpec {
        name: "chat",
        async_job: false,
        request_dto: "ChatRequest",
        request_schema: request_schema::<crate::schema::ChatRequest>,
        scope: ActionScope::Read,
        description: "Send a direct prompt to the configured chat-purpose LLM",
        cost: "moderate",
    },
];
