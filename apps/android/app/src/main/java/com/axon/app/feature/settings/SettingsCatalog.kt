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
    val envGroups = axonEnvSettingGroups

    val configGroups =
        listOf(
            SettingGroup(
                "providers.llm",
                "[providers.llm]",
                "LLM Provider",
                "Non-secret model names for synthesis and direct chat.",
                "brain",
                listOf(
                    SettingField(
                        "synthesis-openai-model",
                        SettingKind.Text,
                        "",
                        "OpenAI-compatible model for RAG synthesis.",
                        "AXON_SYNTHESIS_OPENAI_MODEL",
                    ),
                    SettingField(
                        "chat-openai-model",
                        SettingKind.Text,
                        "",
                        "OpenAI-compatible model for direct Chat mode.",
                        "AXON_CHAT_OPENAI_MODEL",
                    ),
                    SettingField(
                        "synthesis-gemini-model",
                        SettingKind.Text,
                        "",
                        "Gemini model for RAG synthesis.",
                        "AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL",
                    ),
                    SettingField(
                        "chat-gemini-model",
                        SettingKind.Text,
                        "",
                        "Gemini model for direct Chat mode.",
                        "AXON_CHAT_HEADLESS_GEMINI_MODEL",
                    ),
                ),
            ),
            SettingGroup(
                "providers.vector",
                "[providers.vector]",
                "Vector Provider",
                "Qdrant hybrid and HNSW tuning.",
                "search",
                listOf(
                    SettingField("hybrid-enabled", SettingKind.Bool, "true", "Enable RRF hybrid search.", "AXON_HYBRID_SEARCH"),
                    SettingField("hnsw-ef", SettingKind.Int, "128", "HNSW ef for named-mode collections.", "AXON_HNSW_EF_SEARCH"),
                ),
            ),
            SettingGroup(
                "retrieval",
                "[retrieval]",
                "Retrieval",
                "RRF candidate windows.",
                "search",
                listOf(
                    SettingField(
                        "hybrid-candidates",
                        SettingKind.Int,
                        "100",
                        "Candidates per prefetch arm before RRF fusion.",
                        "AXON_HYBRID_CANDIDATES",
                    ),
                    SettingField(
                        "ask-hybrid-candidates",
                        SettingKind.Int,
                        "150",
                        "Hybrid prefetch window for ask.",
                        "AXON_ASK_HYBRID_CANDIDATES",
                    ),
                ),
            ),
            SettingGroup(
                "ask",
                "[ask]",
                "Ask Pipeline",
                "Context assembly, chunk limits, and rerank gating for RAG answers.",
                "ask",
                listOf(
                    SettingField(
                        "max-context-chars",
                        SettingKind.Int,
                        "300000",
                        "Max context chars to the LLM.",
                        "AXON_ASK_MAX_CONTEXT_CHARS",
                    ),
                    SettingField("chunk-limit", SettingKind.Int, "20", "Max chunks returned per ask query.", "AXON_ASK_CHUNK_LIMIT"),
                    SettingField(
                        "candidate-limit",
                        SettingKind.Int,
                        "250",
                        "Max candidate chunks fetched before scoring.",
                        "AXON_ASK_CANDIDATE_LIMIT",
                    ),
                    SettingField("full-docs", SettingKind.Int, "6", "Max full documents in context.", "AXON_ASK_FULL_DOCS"),
                    SettingField("backfill-chunks", SettingKind.Int, "5", "Backfill chunks from top docs.", "AXON_ASK_BACKFILL_CHUNKS"),
                    SettingField(
                        "doc-fetch-concurrency",
                        SettingKind.Int,
                        "4",
                        "Concurrent doc fetches during context build.",
                        "AXON_ASK_DOC_FETCH_CONCURRENCY",
                    ),
                    SettingField(
                        "doc-chunk-limit",
                        SettingKind.Int,
                        "96",
                        "Max chunks per document in context.",
                        "AXON_ASK_DOC_CHUNK_LIMIT",
                    ),
                    SettingField(
                        "min-relevance-score",
                        SettingKind.Float,
                        "0.45",
                        "Min relevance score to include a chunk.",
                        "AXON_ASK_MIN_RELEVANCE_SCORE",
                    ),
                    SettingField(
                        "authoritative-domains",
                        SettingKind.List,
                        "code.claude.com",
                        "Domains treated as authoritative during reranking.",
                        "AXON_ASK_AUTHORITATIVE_DOMAINS",
                    ),
                    SettingField(
                        "authoritative-boost",
                        SettingKind.Float,
                        "0.12",
                        "Boost for authoritative domains.",
                        "AXON_ASK_AUTHORITATIVE_BOOST",
                    ),
                    SettingField(
                        "min-citations-nontrivial",
                        SettingKind.Int,
                        "2",
                        "Min unique citations for non-trivial answers.",
                        "AXON_ASK_MIN_CITATIONS_NONTRIVIAL",
                    ),
                ),
            ),
            SettingGroup(
                "ask.cache",
                "[ask.cache]",
                "Ask Cache",
                "In-process doc-chunk cache.",
                "database",
                listOf(
                    SettingField("enabled", SettingKind.Bool, "false", "Enable ask full-doc fetch cache."),
                    SettingField("max-capacity-bytes", SettingKind.Int, "268435456", "Max cached bytes."),
                    SettingField("ttl-secs", SettingKind.Int, "300", "TTL for cached entries."),
                ),
            ),
            SettingGroup(
                "ask.adaptive",
                "[ask.adaptive]",
                "Ask Adaptive",
                "Full-doc fetch skip gate.",
                "zap",
                listOf(
                    SettingField("fulldoc-skip-enabled", SettingKind.Bool, "false", "Elide full-doc backfill when top-K covers enough."),
                    SettingField("fulldoc-skip-min-urls", SettingKind.Int, "3", "Min unique URLs in reranked top-K."),
                    SettingField("fulldoc-skip-min-chars", SettingKind.Int, "4000", "Min total chunk bytes across top-K."),
                    SettingField("fulldoc-skip-score-delta", SettingKind.Float, "0.15", "Cosine-mode score floor offset."),
                ),
            ),
            SettingGroup(
                "providers.embedding",
                "[providers.embedding]",
                "Embedding Provider",
                "Embedding retry, timeout, and batch tuning.",
                "layers",
                listOf(
                    SettingField("max-retries", SettingKind.Int, "5", "Max retry attempts after initial request.", "TEI_MAX_RETRIES"),
                    SettingField(
                        "request-timeout-ms",
                        SettingKind.Int,
                        "30000",
                        "Per-attempt timeout in milliseconds.",
                        "TEI_REQUEST_TIMEOUT_MS",
                    ),
                    SettingField("batch-size", SettingKind.Int, "128", "Default embedding batch size.", "TEI_MAX_CLIENT_BATCH_SIZE"),
                ),
            ),
            SettingGroup(
                "pipeline",
                "[pipeline]",
                "Pipeline",
                "Unified pipeline concurrency and buffering.",
                "activity",
                listOf(
                    SettingField(
                        "unified-worker-concurrency",
                        SettingKind.Int,
                        "8",
                        "Maximum concurrent unified jobs.",
                        "AXON_UNIFIED_WORKER_CONCURRENCY",
                    ),
                    SettingField(
                        "embed-doc-timeout-secs",
                        SettingKind.Int,
                        "300",
                        "Per-document embed timeout.",
                        "AXON_EMBED_DOC_TIMEOUT_SECS",
                    ),
                    SettingField("queue-summary-secs", SettingKind.Int, "30", "Queue summary interval.", "AXON_QUEUE_SUMMARY_SECS"),
                    SettingField(
                        "qdrant-point-buffer",
                        SettingKind.Int,
                        "256",
                        "Buffered Qdrant points before flush.",
                        "AXON_QDRANT_POINT_BUFFER",
                    ),
                    SettingField(
                        "job-wait-timeout-secs",
                        SettingKind.Int,
                        "300",
                        "Timeout for --wait true polling.",
                        "AXON_JOB_WAIT_TIMEOUT_SECS",
                    ),
                ),
            ),
            SettingGroup(
                "providers.render",
                "[providers.render]",
                "Render Provider",
                "Headless Chrome render path.",
                "globe",
                listOf(
                    SettingField(
                        "user-agent",
                        SettingKind.Text,
                        "Mozilla/5.0 (compatible; Axon/1.0)",
                        "Custom Chrome UA.",
                        "AXON_CHROME_USER_AGENT",
                    ),
                    SettingField("bypass-csp", SettingKind.Bool, "false", "Bypass Content Security Policy."),
                    SettingField("accept-invalid-certs", SettingKind.Bool, "false", "Accept invalid TLS certs."),
                    SettingField("network-idle-timeout-secs", SettingKind.Int, "15", "Wait for network idle."),
                    SettingField("bootstrap-timeout-ms", SettingKind.Int, "3000", "Remote Chrome bootstrap timeout."),
                    SettingField("bootstrap-retries", SettingKind.Int, "2", "Remote Chrome bootstrap retries."),
                ),
            ),
            SettingGroup(
                "providers.fetch",
                "[providers.fetch]",
                "Fetch Provider",
                "HTTP delay, timeout, and retry policy.",
                "globe",
                listOf(
                    SettingField("delay-ms", SettingKind.Int, "0", "Delay between requests."),
                    SettingField("request-timeout-ms", SettingKind.Int, "20000", "Per-request HTTP timeout."),
                    SettingField("retries", SettingKind.Int, "2", "Fetch retry count."),
                    SettingField("retry-backoff-ms", SettingKind.Int, "250", "Retry backoff in milliseconds."),
                ),
            ),
            SettingGroup(
                "crawl",
                "[crawl]",
                "Site Acquisition",
                "Site discovery, sitemap/llms.txt backfill, auto-switch, and DOM ladder.",
                "scrape",
                listOf(
                    SettingField("respect-robots", SettingKind.Bool, "false", "Respect robots.txt directives."),
                    SettingField("min-markdown-chars", SettingKind.Int, "200", "Thin-page markdown threshold."),
                    SettingField("drop-thin-markdown", SettingKind.Bool, "true", "Skip thin pages."),
                    SettingField("discover-sitemaps", SettingKind.Bool, "true", "Backfill from sitemap.xml."),
                    SettingField("sitemap-since-days", SettingKind.Int, "0", "Only backfill recent sitemap URLs."),
                    SettingField("max-sitemaps", SettingKind.Int, "512", "Max sitemap docs to parse."),
                    SettingField("discover-llms-txt", SettingKind.Bool, "true", "Probe /llms.txt."),
                    SettingField("max-llms-txt-urls", SettingKind.Int, "512", "Max URLs from /llms.txt."),
                    SettingField("auto-switch-thin-ratio", SettingKind.Float, "0.60", "Thin ratio that triggers Chrome."),
                    SettingField("auto-switch-min-pages", SettingKind.Int, "10", "Min page count before auto-switch."),
                    SettingField("url-whitelist", SettingKind.List, "", "Only crawl URLs matching regex patterns."),
                    SettingField("max-page-bytes", SettingKind.Int, "0", "Max response size per page."),
                    SettingField("redirect-policy-strict", SettingKind.Bool, "false", "Only follow same-origin redirects."),
                    SettingField(
                        "ladder-strategy1-threshold",
                        SettingKind.Int,
                        "30",
                        "DOM ladder strategy 1 threshold.",
                        "AXON_LADDER_STRATEGY1_THRESHOLD",
                    ),
                    SettingField(
                        "ladder-strategy2-threshold",
                        SettingKind.Int,
                        "200",
                        "DOM ladder strategy 2 threshold.",
                        "AXON_LADDER_STRATEGY2_THRESHOLD",
                    ),
                    SettingField(
                        "ladder-body-multiplier",
                        SettingKind.Float,
                        "2.0",
                        "Body fallback multiplier.",
                        "AXON_LADDER_BODY_MULTIPLIER",
                    ),
                ),
            ),
            SettingGroup(
                "crawl.verticals",
                "[crawl.verticals]",
                "Vertical Extractors",
                "Per-site extractor controls.",
                "braces",
                listOf(
                    SettingField("enabled", SettingKind.Bool, "true", "Enable vertical extractors.", "AXON_ENABLE_VERTICALS"),
                    SettingField(
                        "auto-dispatch-skip",
                        SettingKind.List,
                        "",
                        "Verticals to skip in auto-dispatch.",
                        "AXON_AUTO_DISPATCH_SKIP",
                    ),
                ),
            ),
            SettingGroup(
                "crawl.verticals.cache-ttl-secs",
                "[crawl.verticals.cache-ttl-secs]",
                "Vertical Cache TTL",
                "Per-vertical cache TTL in seconds.",
                "clock",
                listOf(
                    SettingField("github", SettingKind.Int, "86400", "GitHub vertical cache TTL.", "AXON_VERTICAL_CACHE_TTL_GITHUB"),
                    SettingField("reddit", SettingKind.Int, "3600", "Reddit vertical cache TTL.", "AXON_VERTICAL_CACHE_TTL_REDDIT"),
                    SettingField("hn", SettingKind.Int, "21600", "Hacker News vertical cache TTL.", "AXON_VERTICAL_CACHE_TTL_HN"),
                ),
            ),
            SettingGroup(
                "crawl.antibot",
                "[crawl.antibot]",
                "Anti-bot",
                "Akamai / Cloudflare challenge handling.",
                "shield",
                listOf(
                    SettingField(
                        "cookie-warmup",
                        SettingKind.Bool,
                        "true",
                        "Cookie warmup retry on antibot challenge.",
                        "AXON_CHALLENGE_WARMUP",
                    ),
                    SettingField(
                        "max-body-scan-bytes",
                        SettingKind.Int,
                        "150000",
                        "Max bytes scanned for challenge patterns.",
                        "AXON_ANTIBOT_MAX_BODY_SCAN_BYTES",
                    ),
                ),
            ),
            SettingGroup(
                "providers.vector.payload",
                "[providers.vector]",
                "Vector Payload",
                "Qdrant structured-blob payload sizing.",
                "database",
                listOf(
                    SettingField(
                        "structured-data-max-bytes",
                        SettingKind.Int,
                        "65536",
                        "Max structured_blob bytes per chunk.",
                        "AXON_STRUCTURED_DATA_MAX_BYTES",
                    ),
                ),
            ),
        )

    val envDefaults: Map<String, String> = envGroups.flatMap { g -> g.fields.map { it.key to it.defaultValue } }.toMap()
    val envSecretKeys: Set<String> =
        envGroups
            .flatMap { it.fields }
            .filter { it.kind == SettingKind.Secret }
            .map { it.key }
            .toSet()
    val configDefaults: Map<String, String> = configGroups.flatMap { g -> g.fields.map { "${g.id}.${it.key}" to it.defaultValue } }.toMap()
    val configSecretKeys: Set<String> =
        configGroups
            .flatMap { group -> group.fields.map { field -> "${group.id}.${field.key}" to field } }
            .filter { it.second.kind == SettingKind.Secret }
            .map { it.first }
            .toSet()
}
