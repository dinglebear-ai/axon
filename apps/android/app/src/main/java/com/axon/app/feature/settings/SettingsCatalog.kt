package com.axon.app.feature.settings

enum class SettingKind { Text, Secret, Bool, Int, Float, Enum, List }

data class SettingField(
    val key: String,
    val kind: SettingKind,
    val defaultValue: String,
    val desc: String,
    val env: String? = null,
    val options: List<String> = emptyList(),
)

data class SettingGroup(
    val id: String,
    val section: String? = null,
    val label: String,
    val note: String,
    val icon: String,
    val fields: List<SettingField>,
)

object AxonSettingsCatalog {
    val envGroups = listOf(
        SettingGroup(
            id = "urls",
            label = "Data & Service URLs",
            icon = "server",
            note = "Endpoint URLs and the config home. Live in .env, never config.toml.",
            fields = listOf(
                SettingField("AXON_DATA_DIR", SettingKind.Text, "", "Config home: .env, config.toml, jobs.db, output, logs, qdrant, tei."),
                SettingField("AXON_HOME", SettingKind.Text, "", "Axon home root for Docker bind mounts."),
                SettingField("QDRANT_URL", SettingKind.Text, "http://127.0.0.1:53333", "Qdrant vector store endpoint."),
                SettingField("TEI_URL", SettingKind.Text, "http://127.0.0.1:52000", "Hugging Face TEI embeddings endpoint."),
                SettingField("AXON_CHROME_REMOTE_URL", SettingKind.Text, "http://127.0.0.1:6000", "Chrome render / CDP proxy endpoint."),
                SettingField("AXON_COLLECTION", SettingKind.Text, "axon", "Default Qdrant collection name."),
            ),
        ),
        SettingGroup(
            id = "mcp",
            label = "MCP Server & Auth",
            icon = "shield",
            note = "MCP-over-HTTP transport and auth. OAuth fields apply only when auth mode is oauth.",
            fields = listOf(
                SettingField("AXON_MCP_HTTP_HOST", SettingKind.Text, "127.0.0.1", "Bind host for the MCP HTTP server."),
                SettingField("AXON_MCP_HTTP_PORT", SettingKind.Int, "8001", "Bind port for the MCP HTTP server."),
                SettingField("AXON_MCP_HTTP_TOKEN", SettingKind.Secret, "", "Static bearer token. setup init generates this for local bearer mode."),
                SettingField("AXON_MCP_AUTH_MODE", SettingKind.Enum, "bearer", "Auth policy for /mcp and direct /v1 routes.", options = listOf("bearer", "oauth")),
                SettingField("AXON_MCP_PUBLIC_URL", SettingKind.Text, "", "Public base URL advertised to OAuth clients."),
                SettingField("AXON_MCP_GOOGLE_CLIENT_ID", SettingKind.Text, "", "Google OAuth client id."),
                SettingField("AXON_MCP_GOOGLE_CLIENT_SECRET", SettingKind.Secret, "", "Google OAuth client secret."),
                SettingField("AXON_MCP_AUTH_ADMIN_EMAIL", SettingKind.Text, "", "Admin email granted full server scope under OAuth."),
                SettingField("AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS", SettingKind.Text, "", "Comma-separated allowed OAuth redirect URIs."),
                SettingField("AXON_MCP_ALLOWED_ORIGINS", SettingKind.Text, "", "Comma-separated allowed CORS origins."),
            ),
        ),
        SettingGroup(
            id = "ingest",
            label = "Ingest Credentials",
            icon = "key",
            note = "Optional API keys. Each unlocks a source or higher rate limits.",
            fields = listOf(
                SettingField("TAVILY_API_KEY", SettingKind.Secret, "", "Tavily key for search / research."),
                SettingField("AXON_SEARXNG_URL", SettingKind.Text, "", "Self-hosted SearXNG search endpoint. Overrides Tavily for research."),
                SettingField("GITHUB_TOKEN", SettingKind.Secret, "", "Higher-rate GitHub ingest."),
                SettingField("GITLAB_TOKEN", SettingKind.Secret, "", "GitLab repo ingest."),
                SettingField("GITEA_TOKEN", SettingKind.Secret, "", "Gitea repo ingest."),
                SettingField("REDDIT_CLIENT_ID", SettingKind.Secret, "", "Reddit app client id."),
                SettingField("REDDIT_CLIENT_SECRET", SettingKind.Secret, "", "Reddit app client secret."),
                SettingField("HF_TOKEN", SettingKind.Secret, "", "Hugging Face token for gated TEI model pulls."),
            ),
        ),
        SettingGroup(
            id = "llm",
            label = "LLM Runtime",
            icon = "brain",
            note = "Synthesis backend for ask / evaluate / suggest / extract / research.",
            fields = listOf(
                SettingField("AXON_LLM_BACKEND", SettingKind.Enum, "", "Synthesis backend. Empty = Gemini headless.", options = listOf("", "gemini", "openai-compat")),
                SettingField("AXON_OPENAI_BASE_URL", SettingKind.Text, "", "OpenAI-compatible API root. No /chat/completions."),
                SettingField("AXON_SYNTHESIS_OPENAI_MODEL", SettingKind.Text, "", "OpenAI-compatible model for RAG synthesis."),
                SettingField("AXON_OPENAI_MODEL", SettingKind.Text, "", "Legacy alias for the synthesis OpenAI-compatible model."),
                SettingField("AXON_CHAT_OPENAI_MODEL", SettingKind.Text, "", "OpenAI-compatible model for direct Chat mode. Empty uses synthesis model."),
                SettingField("AXON_OPENAI_API_KEY", SettingKind.Secret, "", "Optional key for the OpenAI-compatible endpoint."),
                SettingField("GEMINI_API_KEY", SettingKind.Secret, "", "Gemini API key. Leave blank to use OAuth under HOME/.gemini."),
                SettingField("GEMINI_HOME", SettingKind.Text, "", "Host dir holding Gemini CLI OAuth credentials."),
                SettingField("AXON_HEADLESS_GEMINI_HOME", SettingKind.Text, "", "Dir Axon copies OAuth files FROM per invocation."),
                SettingField("AXON_HEADLESS_GEMINI_CMD", SettingKind.Text, "", "Path to the gemini binary."),
                SettingField("AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL", SettingKind.Text, "", "Gemini model for RAG synthesis."),
                SettingField("AXON_HEADLESS_GEMINI_MODEL", SettingKind.Text, "", "Legacy alias for the synthesis Gemini model."),
                SettingField("AXON_CHAT_HEADLESS_GEMINI_MODEL", SettingKind.Text, "", "Gemini model for direct Chat mode. Empty uses synthesis model."),
            ),
        ),
        SettingGroup(
            id = "http",
            label = "HTTP Behavior",
            icon = "globe",
            note = "User-Agent strings for outbound HTTP and Chrome render paths.",
            fields = listOf(
                SettingField("AXON_USER_AGENT", SettingKind.Text, "", "UA for all HTTP requests."),
                SettingField("AXON_CHROME_USER_AGENT", SettingKind.Text, "", "Chrome-specific UA override."),
            ),
        ),
        SettingGroup(
            id = "logging",
            label = "Logging",
            icon = "file",
            note = "Log rotation is env-only because tracing starts before config.toml is read.",
            fields = listOf(
                SettingField("AXON_LOG_PATH", SettingKind.Text, "", "Active log file path."),
            ),
        ),
        SettingGroup(
            id = "docker",
            label = "Docker / Compose",
            icon = "layers",
            note = "Compose interpolation and TEI/GPU bootstrap.",
            fields = listOf(
                SettingField("AXON_IMAGE", SettingKind.Text, "", "Axon server image tag for Compose."),
                SettingField("AXON_MCP_HTTP_PUBLISH", SettingKind.Int, "8001", "Published host port for MCP HTTP."),
                SettingField("TEI_EMBEDDING_MODEL", SettingKind.Text, "Qwen/Qwen3-Embedding-0.6B", "Production embedding model served by TEI."),
                SettingField("TEI_HTTP_PORT", SettingKind.Int, "52000", "TEI host port."),
                SettingField("TEI_SERVER_MAX_CLIENT_BATCH_SIZE", SettingKind.Int, "96", "TEI server-side max client batch size."),
                SettingField("NVIDIA_VISIBLE_DEVICES", SettingKind.Text, "0", "GPU devices exposed to TEI."),
                SettingField("CUDA_VISIBLE_DEVICES", SettingKind.Text, "0", "CUDA device ordinal."),
            ),
        ),
    )

    val configGroups = listOf(
        SettingGroup("llm", "[llm]", "LLM Models", "Non-secret model names for synthesis and direct chat.", "brain", listOf(
            SettingField("synthesis-openai-model", SettingKind.Text, "", "OpenAI-compatible model for RAG synthesis.", "AXON_SYNTHESIS_OPENAI_MODEL"),
            SettingField("chat-openai-model", SettingKind.Text, "", "OpenAI-compatible model for direct Chat mode.", "AXON_CHAT_OPENAI_MODEL"),
            SettingField("synthesis-gemini-model", SettingKind.Text, "", "Gemini model for RAG synthesis.", "AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL"),
            SettingField("chat-gemini-model", SettingKind.Text, "", "Gemini model for direct Chat mode.", "AXON_CHAT_HEADLESS_GEMINI_MODEL"),
        )),
        SettingGroup("search", "[search]", "Search & Hybrid", "RRF hybrid retrieval and HNSW recall tuning.", "search", listOf(
            SettingField("hybrid-enabled", SettingKind.Bool, "true", "Enable RRF hybrid search.", "AXON_HYBRID_SEARCH"),
            SettingField("hybrid-candidates", SettingKind.Int, "100", "Candidates per prefetch arm before RRF fusion.", "AXON_HYBRID_CANDIDATES"),
            SettingField("ask-hybrid-candidates", SettingKind.Int, "150", "Hybrid prefetch window for ask.", "AXON_ASK_HYBRID_CANDIDATES"),
            SettingField("hnsw-ef", SettingKind.Int, "128", "HNSW ef for named-mode collections.", "AXON_HNSW_EF_SEARCH"),
            SettingField("hnsw-ef-legacy", SettingKind.Int, "64", "HNSW ef for legacy unnamed collections.", "AXON_HNSW_EF_SEARCH_LEGACY"),
            SettingField("collection", SettingKind.Text, "axon", "Default Qdrant collection name.", "AXON_COLLECTION"),
        )),
        SettingGroup("ask", "[ask]", "Ask Pipeline", "Context assembly, chunk limits, and rerank gating for RAG answers.", "ask", listOf(
            SettingField("max-context-chars", SettingKind.Int, "300000", "Max context chars to the LLM.", "AXON_ASK_MAX_CONTEXT_CHARS"),
            SettingField("chunk-limit", SettingKind.Int, "20", "Max chunks returned per ask query.", "AXON_ASK_CHUNK_LIMIT"),
            SettingField("candidate-limit", SettingKind.Int, "250", "Max candidate chunks fetched before scoring.", "AXON_ASK_CANDIDATE_LIMIT"),
            SettingField("full-docs", SettingKind.Int, "6", "Max full documents in context.", "AXON_ASK_FULL_DOCS"),
            SettingField("backfill-chunks", SettingKind.Int, "5", "Backfill chunks from top docs.", "AXON_ASK_BACKFILL_CHUNKS"),
            SettingField("doc-fetch-concurrency", SettingKind.Int, "4", "Concurrent doc fetches during context build.", "AXON_ASK_DOC_FETCH_CONCURRENCY"),
            SettingField("doc-chunk-limit", SettingKind.Int, "96", "Max chunks per document in context.", "AXON_ASK_DOC_CHUNK_LIMIT"),
            SettingField("min-relevance-score", SettingKind.Float, "0.45", "Min relevance score to include a chunk.", "AXON_ASK_MIN_RELEVANCE_SCORE"),
            SettingField("authoritative-domains", SettingKind.List, "code.claude.com", "Domains treated as authoritative during reranking.", "AXON_ASK_AUTHORITATIVE_DOMAINS"),
            SettingField("authoritative-boost", SettingKind.Float, "0.12", "Boost for authoritative domains.", "AXON_ASK_AUTHORITATIVE_BOOST"),
            SettingField("min-citations-nontrivial", SettingKind.Int, "2", "Min unique citations for non-trivial answers.", "AXON_ASK_MIN_CITATIONS_NONTRIVIAL"),
        )),
        SettingGroup("ask.cache", "[ask.cache]", "Ask Cache", "In-process doc-chunk cache.", "database", listOf(
            SettingField("enabled", SettingKind.Bool, "false", "Enable ask full-doc fetch cache."),
            SettingField("max-capacity-bytes", SettingKind.Int, "268435456", "Max cached bytes."),
            SettingField("ttl-secs", SettingKind.Int, "300", "TTL for cached entries."),
        )),
        SettingGroup("ask.adaptive", "[ask.adaptive]", "Ask Adaptive", "Full-doc fetch skip gate.", "zap", listOf(
            SettingField("fulldoc-skip-enabled", SettingKind.Bool, "false", "Elide full-doc backfill when top-K covers enough."),
            SettingField("fulldoc-skip-min-urls", SettingKind.Int, "3", "Min unique URLs in reranked top-K."),
            SettingField("fulldoc-skip-min-chars", SettingKind.Int, "4000", "Min total chunk bytes across top-K."),
            SettingField("fulldoc-skip-score-delta", SettingKind.Float, "0.15", "Cosine-mode score floor offset."),
        )),
        SettingGroup("tei", "[tei]", "TEI Client", "Embeddings client retry, timeout, and batch tuning.", "layers", listOf(
            SettingField("max-retries", SettingKind.Int, "5", "Max retry attempts after initial request.", "TEI_MAX_RETRIES"),
            SettingField("request-timeout-ms", SettingKind.Int, "30000", "Per-attempt timeout in milliseconds.", "TEI_REQUEST_TIMEOUT_MS"),
            SettingField("max-client-batch-size", SettingKind.Int, "64", "Default TEI batch size.", "TEI_MAX_CLIENT_BATCH_SIZE"),
        )),
        SettingGroup("workers", "[workers]", "Workers & Jobs", "Worker lanes, queue caps, concurrency, and watchdog.", "activity", listOf(
            SettingField("ingest-lanes", SettingKind.Int, "2", "Parallel ingest worker lanes.", "AXON_INGEST_LANES"),
            SettingField("embed-lanes", SettingKind.Int, "2", "Parallel embed worker lanes.", "AXON_EMBED_LANES"),
            SettingField("embed-doc-timeout-secs", SettingKind.Int, "300", "Per-document embed timeout.", "AXON_EMBED_DOC_TIMEOUT_SECS"),
            SettingField("queue-summary-secs", SettingKind.Int, "30", "Queue summary interval.", "AXON_QUEUE_SUMMARY_SECS"),
            SettingField("qdrant-point-buffer", SettingKind.Int, "256", "Buffered Qdrant points before flush.", "AXON_QDRANT_POINT_BUFFER"),
            SettingField("max-pending-crawl-jobs", SettingKind.Int, "100", "Reject crawl jobs above this count.", "AXON_MAX_PENDING_CRAWL_JOBS"),
            SettingField("max-pending-embed-jobs", SettingKind.Int, "50", "Reject embed jobs above this count.", "AXON_MAX_PENDING_EMBED_JOBS"),
            SettingField("max-pending-extract-jobs", SettingKind.Int, "50", "Reject extract jobs above this count.", "AXON_MAX_PENDING_EXTRACT_JOBS"),
            SettingField("max-pending-ingest-jobs", SettingKind.Int, "50", "Reject ingest jobs above this count.", "AXON_MAX_PENDING_INGEST_JOBS"),
            SettingField("job-wait-timeout-secs", SettingKind.Int, "300", "Timeout for --wait true polling.", "AXON_JOB_WAIT_TIMEOUT_SECS"),
            SettingField("concurrency-limit", SettingKind.Int, "128", "Override crawl and backfill concurrency at once."),
            SettingField("crawl-concurrency-limit", SettingKind.Int, "128", "Override crawl concurrency."),
            SettingField("backfill-concurrency-limit", SettingKind.Int, "64", "Override sitemap backfill concurrency."),
            SettingField("watchdog-stale-timeout-secs", SettingKind.Int, "300", "Seconds before a running job is stale.", "AXON_JOB_STALE_TIMEOUT_SECS"),
            SettingField("watchdog-confirm-secs", SettingKind.Int, "60", "Grace period before stale reclaim.", "AXON_JOB_STALE_CONFIRM_SECS"),
            SettingField("watchdog-sweep-secs", SettingKind.Int, "15", "Seconds between watchdog sweeps.", "AXON_WATCHDOG_SWEEP_SECS"),
        )),
        SettingGroup("chrome", "[chrome]", "Chrome Render", "Headless Chrome render path.", "globe", listOf(
            SettingField("user-agent", SettingKind.Text, "Mozilla/5.0 (compatible; Axon/1.0)", "Custom Chrome UA.", "AXON_CHROME_USER_AGENT"),
            SettingField("bypass-csp", SettingKind.Bool, "false", "Bypass Content Security Policy."),
            SettingField("accept-invalid-certs", SettingKind.Bool, "false", "Accept invalid TLS certs."),
            SettingField("network-idle-timeout-secs", SettingKind.Int, "15", "Wait for network idle."),
            SettingField("bootstrap-timeout-ms", SettingKind.Int, "3000", "Remote Chrome bootstrap timeout."),
            SettingField("bootstrap-retries", SettingKind.Int, "2", "Remote Chrome bootstrap retries."),
        )),
        SettingGroup("scrape", "[scrape]", "Scrape & Crawl", "Fetch behavior, sitemap/llms.txt backfill, auto-switch, and DOM ladder.", "scrape", listOf(
            SettingField("respect-robots", SettingKind.Bool, "false", "Respect robots.txt directives."),
            SettingField("min-markdown-chars", SettingKind.Int, "200", "Thin-page markdown threshold."),
            SettingField("drop-thin-markdown", SettingKind.Bool, "true", "Skip thin pages."),
            SettingField("discover-sitemaps", SettingKind.Bool, "true", "Backfill from sitemap.xml."),
            SettingField("sitemap-since-days", SettingKind.Int, "0", "Only backfill recent sitemap URLs."),
            SettingField("max-sitemaps", SettingKind.Int, "512", "Max sitemap docs to parse."),
            SettingField("discover-llms-txt", SettingKind.Bool, "true", "Probe /llms.txt."),
            SettingField("max-llms-txt-urls", SettingKind.Int, "512", "Max URLs from /llms.txt."),
            SettingField("delay-ms", SettingKind.Int, "0", "Delay between requests."),
            SettingField("request-timeout-ms", SettingKind.Int, "20000", "Per-request HTTP timeout."),
            SettingField("fetch-retries", SettingKind.Int, "2", "Fetch retry count."),
            SettingField("retry-backoff-ms", SettingKind.Int, "250", "Retry backoff in milliseconds."),
            SettingField("auto-switch-thin-ratio", SettingKind.Float, "0.60", "Thin ratio that triggers Chrome."),
            SettingField("auto-switch-min-pages", SettingKind.Int, "10", "Min page count before auto-switch."),
            SettingField("url-whitelist", SettingKind.List, "", "Only crawl URLs matching regex patterns."),
            SettingField("max-page-bytes", SettingKind.Int, "0", "Max response size per page."),
            SettingField("redirect-policy-strict", SettingKind.Bool, "false", "Only follow same-origin redirects."),
            SettingField("ladder-strategy1-threshold", SettingKind.Int, "30", "DOM ladder strategy 1 threshold.", "AXON_LADDER_STRATEGY1_THRESHOLD"),
            SettingField("ladder-strategy2-threshold", SettingKind.Int, "200", "DOM ladder strategy 2 threshold.", "AXON_LADDER_STRATEGY2_THRESHOLD"),
            SettingField("ladder-body-multiplier", SettingKind.Float, "2.0", "Body fallback multiplier.", "AXON_LADDER_BODY_MULTIPLIER"),
        )),
        SettingGroup("verticals", "[verticals]", "Vertical Extractors", "Per-site extractor controls.", "braces", listOf(
            SettingField("enabled", SettingKind.Bool, "true", "Enable vertical extractors.", "AXON_ENABLE_VERTICALS"),
            SettingField("auto-dispatch-skip", SettingKind.List, "", "Verticals to skip in auto-dispatch.", "AXON_AUTO_DISPATCH_SKIP"),
        )),
        SettingGroup("verticals.cache-ttl-secs", "[verticals.cache-ttl-secs]", "Vertical Cache TTL", "Per-vertical cache TTL in seconds.", "clock", listOf(
            SettingField("github", SettingKind.Int, "86400", "GitHub vertical cache TTL.", "AXON_VERTICAL_CACHE_TTL_GITHUB"),
            SettingField("reddit", SettingKind.Int, "3600", "Reddit vertical cache TTL.", "AXON_VERTICAL_CACHE_TTL_REDDIT"),
            SettingField("hn", SettingKind.Int, "21600", "Hacker News vertical cache TTL.", "AXON_VERTICAL_CACHE_TTL_HN"),
        )),
        SettingGroup("antibot", "[antibot]", "Anti-bot", "Akamai / Cloudflare challenge handling.", "shield", listOf(
            SettingField("cookie-warmup", SettingKind.Bool, "true", "Cookie warmup retry on antibot challenge.", "AXON_CHALLENGE_WARMUP"),
            SettingField("max-body-scan-bytes", SettingKind.Int, "150000", "Max bytes scanned for challenge patterns.", "AXON_ANTIBOT_MAX_BODY_SCAN_BYTES"),
        )),
        SettingGroup("payload", "[payload]", "Payload", "Qdrant structured-blob payload sizing.", "database", listOf(
            SettingField("structured-data-max-bytes", SettingKind.Int, "65536", "Max structured_blob bytes per chunk.", "AXON_STRUCTURED_DATA_MAX_BYTES"),
        )),
    )

    val envDefaults: Map<String, String> = envGroups.flatMap { g -> g.fields.map { it.key to it.defaultValue } }.toMap()
    val envSecretKeys: Set<String> = envGroups
        .flatMap { it.fields }
        .filter { it.kind == SettingKind.Secret }
        .map { it.key }
        .toSet()
    val configDefaults: Map<String, String> = configGroups.flatMap { g -> g.fields.map { "${g.id}.${it.key}" to it.defaultValue } }.toMap()
    val configSecretKeys: Set<String> = configGroups
        .flatMap { group -> group.fields.map { field -> "${group.id}.${field.key}" to field } }
        .filter { it.second.kind == SettingKind.Secret }
        .map { it.first }
        .toSet()
}
