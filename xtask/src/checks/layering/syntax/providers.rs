use super::path_has_prefix;

const PROVIDER_TYPES: &[&str] = &[
    "EmbeddingProvider",
    "VectorStore",
    "SearchProvider",
    "FetchProvider",
    "RenderProvider",
    "NetworkCaptureProvider",
    "GraphStore",
    "ArtifactStore",
    "LlmProvider",
];

const CONCRETE_PROVIDER_TYPES: &[&str] = &[
    "QdrantVectorStore",
    "FakeVectorStore",
    "TeiEmbeddingProvider",
    "OpenAiCompatEmbeddingProvider",
    "FakeEmbeddingProvider",
    "SearxngSearchProvider",
    "TavilySearchProvider",
    "HttpFetchProvider",
    "ChromeRenderProvider",
    "FakeAdapterProviders",
    "ChromeNetworkCapture",
    "FileArtifactStore",
    "FakeCoreBoundaries",
    "SqliteGraphStore",
    "FakeGraphStore",
    "FakeLlmProvider",
];

pub(super) const PROVIDER_HANDLES: &[&str] = &[
    "embedding_provider",
    "vector_store",
    "search_provider",
    "fetch_provider",
    "render_provider",
    "network_capture_provider",
    "capture_provider",
    "graph_store",
    "artifact_store",
    "llm_provider",
];

// Names specific enough to reject independent of receiver type. Collision-
// prone operations remain enforceable whenever the receiver handle/type or
// provider-qualified UFCS path is present.
pub(super) const LOW_COLLISION_PROVIDER_METHODS: &[&str] = &[
    "embed",
    "ensure_collection",
    "mark_generation_committed",
    "mark_unchanged_items_committed",
    "upsert_candidates",
    "put_bytes",
    "complete_streaming",
    "node_edges",
    "nodes_for_source",
    "delete_nodes",
    "delete_edges",
];

pub(super) const PROVIDER_GLOB_ROOTS: &[&str] = &[
    "axon_adapters",
    "axon_embedding",
    "axon_graph",
    "axon_llm",
    "axon_vectors",
];

const PROVIDER_IMPLEMENTATION_PREFIXES: &[&[&str]] = &[
    &["axon_vectors", "qdrant"],
    &["axon_embedding", "tei"],
    &["axon_embedding", "openai_compat"],
    &["axon_adapters", "providers", "http_fetch"],
    &["axon_adapters", "providers", "chrome_render"],
    &["axon_adapters", "providers", "searxng_search"],
    &["axon_adapters", "providers", "tavily_search"],
    &["axon_core", "boundary", "file_artifact_store"],
    &["axon_graph", "sqlite"],
    &["axon_llm", "fake"],
];

pub(super) fn is_provider_type_name(name: &str) -> bool {
    PROVIDER_TYPES.contains(&name) || CONCRETE_PROVIDER_TYPES.contains(&name)
}

pub(super) fn is_provider_path(path: &[String]) -> bool {
    path.last().is_some_and(|name| is_provider_type_name(name))
        || path.last().is_some_and(|name| {
            name.chars().next().is_some_and(char::is_uppercase)
                && PROVIDER_IMPLEMENTATION_PREFIXES
                    .iter()
                    .any(|prefix| path_has_prefix(path, prefix))
        })
}
