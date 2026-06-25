export type ConfigFieldType = "text" | "secret" | "bool" | "int" | "float" | "enum" | "list";

export interface ConfigField {
  key: string;
  type: ConfigFieldType;
  def: string | number | boolean | string[];
  desc: string;
  env?: string;
  options?: string[];
}

export interface EnvGroup {
  id: string;
  label: string;
  icon: string;
  note: string;
  vars: ConfigField[];
}

export interface TomlConfigGroup {
  id: string;
  section: string;
  label: string;
  icon: string;
  note: string;
  knobs: ConfigField[];
}

/* ============================================================
 * Axon — configuration model
 * The full env (~/.axon/.env) + config.toml knob surface, mirrored
 * from the repo's .env.example and config.example.toml so Settings
 * can render every knob. Data only; panels render it generically.
 *
 * field.type ∈ text | secret | bool | int | float | enum | list
 * env-layer fields carry { key }; config.toml knobs carry { key, env }.
 * ============================================================ */

/* ── ENV layer — ~/.axon/.env : URLs, secrets, auth, runtime bootstrap ── */
export const ENV_GROUPS: EnvGroup[] = [
  {
    id: 'urls', label: 'Data & Service URLs', icon: 'server',
    note: 'Endpoint URLs and the config home. Live in .env, never config.toml.',
    vars: [
      { key: 'AXON_DATA_DIR', type: 'text', def: '~/.axon', desc: 'Config home: .env, config.toml, jobs.db, output, logs, qdrant, tei.' },
      { key: 'AXON_HOME', type: 'text', def: '~/.axon', desc: 'Axon home root for Docker bind mounts.' },
      { key: 'QDRANT_URL', type: 'text', def: 'http://127.0.0.1:53333', desc: 'Qdrant vector store endpoint.' },
      { key: 'TEI_URL', type: 'text', def: 'http://127.0.0.1:52000', desc: 'Hugging Face TEI embeddings endpoint (Qwen3).' },
      { key: 'AXON_CHROME_REMOTE_URL', type: 'text', def: 'http://127.0.0.1:6000', desc: 'Chrome render / CDP proxy endpoint.' },
      { key: 'AXON_COLLECTION', type: 'text', def: 'axon', desc: 'Default Qdrant collection name.' },
    ],
  },
  {
    id: 'mcp', label: 'MCP Server & Auth', icon: 'shield',
    note: 'MCP-over-HTTP transport and auth. OAuth fields apply only when auth mode is oauth.',
    vars: [
      { key: 'AXON_MCP_HTTP_HOST', type: 'text', def: '127.0.0.1', desc: 'Bind host for the MCP HTTP server.' },
      { key: 'AXON_MCP_HTTP_PORT', type: 'int', def: '8001', desc: 'Bind port for the MCP HTTP server.' },
      { key: 'AXON_MCP_HTTP_TOKEN', type: 'secret', def: '', desc: 'Static bearer token. setup init generates this for local bearer mode.' },
      { key: 'AXON_MCP_AUTH_MODE', type: 'enum', options: ['bearer', 'oauth'], def: 'bearer', desc: 'Auth policy for /mcp and direct /v1 routes.' },
      { key: 'AXON_MCP_PUBLIC_URL', type: 'text', def: '', desc: 'Public base URL advertised to OAuth clients.' },
      { key: 'AXON_MCP_GOOGLE_CLIENT_ID', type: 'text', def: '', desc: 'Google OAuth client id.' },
      { key: 'AXON_MCP_GOOGLE_CLIENT_SECRET', type: 'secret', def: '', desc: 'Google OAuth client secret.' },
      { key: 'AXON_MCP_AUTH_ADMIN_EMAIL', type: 'text', def: '', desc: 'Admin email granted full server scope under OAuth.' },
      { key: 'AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS', type: 'text', def: '', desc: 'Comma-separated allowed OAuth redirect URIs.' },
      { key: 'AXON_MCP_ALLOWED_ORIGINS', type: 'text', def: '', desc: 'Comma-separated allowed CORS origins.' },
    ],
  },
  {
    id: 'ingest', label: 'Ingest Credentials', icon: 'key',
    note: 'Optional API keys. Each unlocks a source or higher rate limits.',
    vars: [
      { key: 'TAVILY_API_KEY', type: 'secret', def: '', desc: 'Tavily key for search / research.' },
      { key: 'AXON_SEARXNG_URL', type: 'text', def: '', desc: 'Self-hosted SearXNG (JSON enabled). Overrides Tavily for research.' },
      { key: 'GITHUB_TOKEN', type: 'secret', def: '', desc: 'Higher-rate GitHub ingest (code, issues, PRs, wiki).' },
      { key: 'GITLAB_TOKEN', type: 'secret', def: '', desc: 'GitLab repo ingest.' },
      { key: 'GITEA_TOKEN', type: 'secret', def: '', desc: 'Gitea repo ingest.' },
      { key: 'REDDIT_CLIENT_ID', type: 'secret', def: '', desc: 'Reddit app client id for subreddit ingest.' },
      { key: 'REDDIT_CLIENT_SECRET', type: 'secret', def: '', desc: 'Reddit app client secret.' },
      { key: 'HF_TOKEN', type: 'secret', def: '', desc: 'Hugging Face token for gated TEI model pulls.' },
    ],
  },
  {
    id: 'llm', label: 'LLM Runtime', icon: 'brain',
    note: 'Synthesis backend for ask / evaluate / suggest / extract / research. Default: Gemini headless.',
    vars: [
      { key: 'AXON_LLM_BACKEND', type: 'enum', options: ['gemini', 'openai-compat'], def: 'gemini', desc: 'Synthesis backend. Empty = Gemini headless.' },
      { key: 'AXON_OPENAI_BASE_URL', type: 'text', def: '', desc: 'OpenAI-compatible API root (llama.cpp, LM Studio). No /chat/completions.' },
      { key: 'AXON_OPENAI_MODEL', type: 'text', def: '', desc: 'Model name for the OpenAI-compatible endpoint.' },
      { key: 'AXON_OPENAI_API_KEY', type: 'secret', def: '', desc: 'Optional key for the OpenAI-compatible endpoint.' },
      { key: 'GEMINI_API_KEY', type: 'secret', def: '', desc: 'Gemini API key. Leave blank to use OAuth under $HOME/.gemini.' },
      { key: 'GEMINI_HOME', type: 'text', def: '', desc: 'Host dir holding Gemini CLI OAuth credentials (Docker mount).' },
      { key: 'AXON_HEADLESS_GEMINI_HOME', type: 'text', def: '$HOME', desc: 'Dir Axon copies OAuth files FROM per invocation.' },
      { key: 'AXON_HEADLESS_GEMINI_CMD', type: 'text', def: 'gemini', desc: 'Path to the gemini binary.' },
      { key: 'AXON_HEADLESS_GEMINI_MODEL', type: 'text', def: '', desc: 'Gemini model override (e.g. gemini-2.5-flash-exp).' },
    ],
  },
  {
    id: 'http', label: 'HTTP Behavior', icon: 'globe',
    note: 'User-Agent strings for all outbound HTTP and Chrome render paths.',
    vars: [
      { key: 'AXON_USER_AGENT', type: 'text', def: '', desc: 'UA for all HTTP requests. Falls back to a Firefox UA.' },
      { key: 'AXON_CHROME_USER_AGENT', type: 'text', def: '', desc: 'Chrome-specific UA. Falls back to AXON_USER_AGENT.' },
    ],
  },
  {
    id: 'logging', label: 'Logging', icon: 'file',
    note: 'Log rotation is env-only — init_tracing() runs before config.toml is read.',
    vars: [
      { key: 'AXON_LOG_PATH', type: 'text', def: '$AXON_DATA_DIR/logs/axon.log', desc: 'Active log file. Rotated siblings (.1, .2…) live alongside.' },
      { key: 'AXON_LOG_MAX_BYTES', type: 'int', def: '10485760', desc: 'Rotation threshold in bytes (10 MiB). 0 disables rotation.' },
    ],
  },
  {
    id: 'docker', label: 'Docker / Compose', icon: 'layers',
    note: 'Compose interpolation and TEI/GPU bootstrap. Docker path only.',
    vars: [
      { key: 'AXON_IMAGE', type: 'text', def: '', desc: 'Axon server image tag for Compose.' },
      { key: 'AXON_MCP_HTTP_PUBLISH', type: 'int', def: '8001', desc: 'Published host port for the MCP HTTP server.' },
      { key: 'TEI_EMBEDDING_MODEL', type: 'text', def: 'Qwen/Qwen3-Embedding-0.6B', desc: 'Production embedding model served by TEI.' },
      { key: 'TEI_HTTP_PORT', type: 'int', def: '52000', desc: 'TEI host port.' },
      { key: 'TEI_SERVER_MAX_CLIENT_BATCH_SIZE', type: 'int', def: '96', desc: 'TEI server-side max client batch size.' },
      { key: 'NVIDIA_VISIBLE_DEVICES', type: 'text', def: '0', desc: 'GPU device(s) exposed to the TEI container.' },
      { key: 'CUDA_VISIBLE_DEVICES', type: 'text', def: '0', desc: 'CUDA device ordinal.' },
    ],
  },
];

/* ── CONFIG layer — ~/.axon/config.toml : non-secret tuning ── */
export const CONFIG_GROUPS: TomlConfigGroup[] = [
  {
    id: 'search', section: '[search]', label: 'Search & Hybrid', icon: 'search',
    note: 'RRF hybrid retrieval and HNSW recall tuning.',
    knobs: [
      { key: 'hybrid-enabled', type: 'bool', def: true, env: 'AXON_HYBRID_SEARCH', desc: 'Enable RRF hybrid search (needs a named-mode collection).' },
      { key: 'hybrid-candidates', type: 'int', def: 100, env: 'AXON_HYBRID_CANDIDATES', desc: 'Candidates per prefetch arm before RRF fusion (10–500).' },
      { key: 'ask-hybrid-candidates', type: 'int', def: 150, env: 'AXON_ASK_HYBRID_CANDIDATES', desc: 'Hybrid prefetch window for the ask pipeline (10–500).' },
      { key: 'hnsw-ef', type: 'int', def: 128, env: 'AXON_HNSW_EF_SEARCH', desc: 'HNSW ef for named-mode collections (32–512).' },
      { key: 'hnsw-ef-legacy', type: 'int', def: 64, env: 'AXON_HNSW_EF_SEARCH_LEGACY', desc: 'HNSW ef for legacy unnamed-mode collections (16–256).' },
      { key: 'collection', type: 'text', def: 'axon', env: 'AXON_COLLECTION', desc: 'Default Qdrant collection name.' },
    ],
  },
  {
    id: 'ask', section: '[ask]', label: 'Ask Pipeline', icon: 'ask',
    note: 'Context assembly, chunk limits, and rerank gating for RAG answers.',
    knobs: [
      { key: 'max-context-chars', type: 'int', def: 300000, env: 'AXON_ASK_MAX_CONTEXT_CHARS', desc: 'Max context chars to the LLM (20k–1M).' },
      { key: 'chunk-limit', type: 'int', def: 20, env: 'AXON_ASK_CHUNK_LIMIT', desc: 'Max chunks returned per ask query (3–40).' },
      { key: 'candidate-limit', type: 'int', def: 250, env: 'AXON_ASK_CANDIDATE_LIMIT', desc: 'Max candidate chunks fetched before scoring (8–300).' },
      { key: 'full-docs', type: 'int', def: 6, env: 'AXON_ASK_FULL_DOCS', desc: 'Max full documents in context (1–20).' },
      { key: 'backfill-chunks', type: 'int', def: 5, env: 'AXON_ASK_BACKFILL_CHUNKS', desc: 'Backfill chunks from top docs to pad context (0–20).' },
      { key: 'doc-fetch-concurrency', type: 'int', def: 4, env: 'AXON_ASK_DOC_FETCH_CONCURRENCY', desc: 'Concurrent doc fetches during context build (1–16).' },
      { key: 'doc-chunk-limit', type: 'int', def: 96, env: 'AXON_ASK_DOC_CHUNK_LIMIT', desc: 'Max chunks per document in context (8–2000).' },
      { key: 'min-relevance-score', type: 'float', def: 0.45, env: 'AXON_ASK_MIN_RELEVANCE_SCORE', desc: 'Min relevance score to include a chunk (-1.0–2.0).' },
      { key: 'authoritative-domains', type: 'list', def: ['code.claude.com'], env: 'AXON_ASK_AUTHORITATIVE_DOMAINS', desc: 'Domains treated as authoritative during reranking.' },
      { key: 'authoritative-boost', type: 'float', def: 0.12, env: 'AXON_ASK_AUTHORITATIVE_BOOST', desc: 'Boost weight for authoritative domains (0.0–0.5).' },
      { key: 'min-citations-nontrivial', type: 'int', def: 2, env: 'AXON_ASK_MIN_CITATIONS_NONTRIVIAL', desc: 'Min unique citations for non-trivial answers (1–5).' },
    ],
  },
  {
    id: 'ask-cache', section: '[ask.cache]', label: 'Ask Cache', icon: 'database',
    note: 'In-process doc-chunk cache. Only useful in long-lived parents (serve / mcp).',
    knobs: [
      { key: 'enabled', type: 'bool', def: false, desc: 'Enable the ask full-doc fetch cache.' },
      { key: 'max-capacity-bytes', type: 'int', def: 268435456, desc: 'Max cached bytes (default 256 MiB).' },
      { key: 'ttl-secs', type: 'int', def: 300, desc: 'TTL for cached entries (hard-capped at 300s).' },
    ],
  },
  {
    id: 'ask-adaptive', section: '[ask.adaptive]', label: 'Ask Adaptive', icon: 'zap',
    note: 'Full-doc fetch skip gate. Enable only if evaluate shows no quality regression.',
    knobs: [
      { key: 'fulldoc-skip-enabled', type: 'bool', def: false, desc: 'Elide the full-doc backfill when top-K already covers enough.' },
      { key: 'fulldoc-skip-min-urls', type: 'int', def: 3, desc: 'Min unique URLs required in reranked top-K (1–50).' },
      { key: 'fulldoc-skip-min-chars', type: 'int', def: 4000, desc: 'Min total chunk bytes across top-K (500–200k).' },
      { key: 'fulldoc-skip-score-delta', type: 'float', def: 0.15, desc: 'Cosine-mode score floor offset (0.0–1.0). Ignored on RRF.' },
    ],
  },
  {
    id: 'tei', section: '[tei]', label: 'TEI Client', icon: 'layers',
    note: 'Embeddings client retry, timeout, and batch tuning.',
    knobs: [
      { key: 'max-retries', type: 'int', def: 5, env: 'TEI_MAX_RETRIES', desc: 'Max retry attempts after the initial TEI request.' },
      { key: 'request-timeout-ms', type: 'int', def: 30000, env: 'TEI_REQUEST_TIMEOUT_MS', desc: 'Per-attempt timeout in milliseconds.' },
      { key: 'max-client-batch-size', type: 'int', def: 64, env: 'TEI_MAX_CLIENT_BATCH_SIZE', desc: 'Default batch size; auto-splits on HTTP 413.' },
    ],
  },
  {
    id: 'workers', section: '[workers]', label: 'Workers & Jobs', icon: 'activity',
    note: 'Worker lanes, queue caps, concurrency, and the stale-job watchdog.',
    knobs: [
      { key: 'ingest-lanes', type: 'int', def: 2, env: 'AXON_INGEST_LANES', desc: 'Parallel ingest worker lanes (1–16).' },
      { key: 'embed-lanes', type: 'int', def: 2, env: 'AXON_EMBED_LANES', desc: 'Parallel embed worker lanes (1–32).' },
      { key: 'embed-doc-timeout-secs', type: 'int', def: 300, env: 'AXON_EMBED_DOC_TIMEOUT_SECS', desc: 'Per-document embed timeout in seconds.' },
      { key: 'queue-summary-secs', type: 'int', def: 30, env: 'AXON_QUEUE_SUMMARY_SECS', desc: 'Queue summary interval (0 disables, 0–3600).' },
      { key: 'qdrant-point-buffer', type: 'int', def: 256, env: 'AXON_QDRANT_POINT_BUFFER', desc: 'Buffered Qdrant points before flush (128–16384).' },
      { key: 'max-pending-crawl-jobs', type: 'int', def: 100, env: 'AXON_MAX_PENDING_CRAWL_JOBS', desc: 'Reject new crawl jobs above this count (0 = unlimited).' },
      { key: 'max-pending-embed-jobs', type: 'int', def: 50, env: 'AXON_MAX_PENDING_EMBED_JOBS', desc: 'Reject new embed jobs above this count (0 = unlimited).' },
      { key: 'max-pending-extract-jobs', type: 'int', def: 50, env: 'AXON_MAX_PENDING_EXTRACT_JOBS', desc: 'Reject new extract jobs above this count (0 = unlimited).' },
      { key: 'max-pending-ingest-jobs', type: 'int', def: 50, env: 'AXON_MAX_PENDING_INGEST_JOBS', desc: 'Reject new ingest jobs above this count (0 = unlimited).' },
      { key: 'job-wait-timeout-secs', type: 'int', def: 300, env: 'AXON_JOB_WAIT_TIMEOUT_SECS', desc: 'Timeout for --wait true polling (30–3600).' },
      { key: 'concurrency-limit', type: 'int', def: 128, desc: 'Override crawl + backfill concurrency at once.' },
      { key: 'crawl-concurrency-limit', type: 'int', def: 128, desc: 'Override crawl concurrency.' },
      { key: 'backfill-concurrency-limit', type: 'int', def: 64, desc: 'Override sitemap backfill concurrency.' },
      { key: 'watchdog-stale-timeout-secs', type: 'int', def: 300, env: 'AXON_JOB_STALE_TIMEOUT_SECS', desc: 'Seconds before a running job is considered stale.' },
      { key: 'watchdog-confirm-secs', type: 'int', def: 60, env: 'AXON_JOB_STALE_CONFIRM_SECS', desc: 'Grace period before stale reclaim.' },
      { key: 'watchdog-sweep-secs', type: 'int', def: 15, env: 'AXON_WATCHDOG_SWEEP_SECS', desc: 'Seconds between watchdog sweeps.' },
    ],
  },
  {
    id: 'chrome', section: '[chrome]', label: 'Chrome Render', icon: 'globe',
    note: 'Headless Chrome render path — UA, TLS, idle, and bootstrap probes.',
    knobs: [
      { key: 'user-agent', type: 'text', def: '', env: 'AXON_CHROME_USER_AGENT', desc: 'Custom Chrome UA. Unset = Spider default.' },
      { key: 'bypass-csp', type: 'bool', def: false, desc: 'Bypass Content Security Policy in Chrome.' },
      { key: 'accept-invalid-certs', type: 'bool', def: false, desc: 'Accept invalid / self-signed TLS certs.' },
      { key: 'network-idle-timeout-secs', type: 'int', def: 15, desc: 'Wait for network idle before page capture.' },
      { key: 'bootstrap-timeout-ms', type: 'int', def: 3000, desc: 'Remote Chrome bootstrap probe timeout (≥250).' },
      { key: 'bootstrap-retries', type: 'int', def: 2, desc: 'Remote Chrome bootstrap probe retries (0–10).' },
    ],
  },
  {
    id: 'scrape', section: '[scrape]', label: 'Scrape & Crawl', icon: 'scrape',
    note: 'Fetch behavior, sitemap/llms.txt backfill, auto-switch, and the DOM retry ladder.',
    knobs: [
      { key: 'respect-robots', type: 'bool', def: false, desc: 'Respect robots.txt directives.' },
      { key: 'min-markdown-chars', type: 'int', def: 200, desc: 'Thin-page threshold in markdown characters.' },
      { key: 'drop-thin-markdown', type: 'bool', def: true, desc: 'Skip thin pages instead of saving / embedding.' },
      { key: 'discover-sitemaps', type: 'bool', def: true, desc: 'Backfill URLs from sitemap.xml after crawl.' },
      { key: 'sitemap-since-days', type: 'int', def: 0, desc: 'Only backfill sitemap URLs newer than N days (0 = off).' },
      { key: 'max-sitemaps', type: 'int', def: 512, desc: 'Max sitemap documents to parse (0 = unlimited).' },
      { key: 'discover-llms-txt', type: 'bool', def: true, desc: 'Probe /llms.txt and merge its links into discovery.' },
      { key: 'max-llms-txt-urls', type: 'int', def: 512, desc: 'Max URLs from a single /llms.txt (0 = unlimited).' },
      { key: 'delay-ms', type: 'int', def: 0, desc: 'Delay between requests in milliseconds.' },
      { key: 'request-timeout-ms', type: 'int', def: 20000, desc: 'Per-request HTTP timeout.' },
      { key: 'fetch-retries', type: 'int', def: 2, desc: 'Fetch retry count.' },
      { key: 'retry-backoff-ms', type: 'int', def: 250, desc: 'Retry backoff in milliseconds.' },
      { key: 'auto-switch-thin-ratio', type: 'float', def: 0.60, desc: 'Thin-page ratio that triggers auto-switch to Chrome.' },
      { key: 'auto-switch-min-pages', type: 'int', def: 10, desc: 'Min page count before auto-switch eligibility.' },
      { key: 'url-whitelist', type: 'list', def: [], desc: 'Only crawl URLs matching these regex patterns.' },
      { key: 'max-page-bytes', type: 'int', def: 0, desc: 'Max response size per page (0 = unlimited).' },
      { key: 'redirect-policy-strict', type: 'bool', def: false, desc: 'Only follow same-origin redirects.' },
      { key: 'ladder-strategy1-threshold', type: 'int', def: 30, env: 'AXON_LADDER_STRATEGY1_THRESHOLD', desc: 'Ladder S1 word threshold → retry with only_main_content=false (1–1000).' },
      { key: 'ladder-strategy2-threshold', type: 'int', def: 200, env: 'AXON_LADDER_STRATEGY2_THRESHOLD', desc: 'Ladder S2 word threshold → retry with body selector (1–10000).' },
      { key: 'ladder-body-multiplier', type: 'float', def: 2.0, env: 'AXON_LADDER_BODY_MULTIPLIER', desc: 'Body wins only if it yields N× scored words (1.0–10.0).' },
    ],
  },
  {
    id: 'verticals', section: '[verticals]', label: 'Vertical Extractors', icon: 'braces',
    note: 'Per-site extractors (GitHub, PyPI, Reddit, HN…) with per-vertical cache TTLs.',
    knobs: [
      { key: 'enabled', type: 'bool', def: true, env: 'AXON_ENABLE_VERTICALS', desc: 'Enable per-site vertical extractors.' },
      { key: 'auto-dispatch-skip', type: 'list', def: [], env: 'AXON_AUTO_DISPATCH_SKIP', desc: 'Verticals to skip in auto-dispatch (still available explicitly).' },
    ],
  },
  {
    id: 'antibot', section: '[antibot]', label: 'Anti-bot', icon: 'shield',
    note: 'Akamai / Cloudflare challenge handling.',
    knobs: [
      { key: 'cookie-warmup', type: 'bool', def: true, env: 'AXON_CHALLENGE_WARMUP', desc: 'Cookie warmup retry on antibot challenge.' },
      { key: 'max-body-scan-bytes', type: 'int', def: 150000, env: 'AXON_ANTIBOT_MAX_BODY_SCAN_BYTES', desc: 'Max bytes scanned for challenge patterns (1k–10 MiB).' },
    ],
  },
  {
    id: 'payload', section: '[payload]', label: 'Payload', icon: 'database',
    note: 'Qdrant structured-blob payload sizing.',
    knobs: [
      { key: 'structured-data-max-bytes', type: 'int', def: 65536, env: 'AXON_STRUCTURED_DATA_MAX_BYTES', desc: 'Max structured_blob bytes per chunk (1 KiB–16 MiB).' },
    ],
  },
];

/* default value maps for controlled inputs */
export const ENV_DEFAULTS = Object.fromEntries(ENV_GROUPS.flatMap((g) => g.vars.map((v) => [v.key, v.def])));
export const CONFIG_DEFAULTS = Object.fromEntries(
  CONFIG_GROUPS.flatMap((g) => g.knobs.map((k) => [`${g.section.replace(/^\[/, "").replace(/\]$/, "")}.${k.key}`, k.def])),
);

export const ENV_COUNT = ENV_GROUPS.reduce((n, g) => n + g.vars.length, 0);
export const CONFIG_COUNT = CONFIG_GROUPS.reduce((n, g) => n + g.knobs.length, 0);
