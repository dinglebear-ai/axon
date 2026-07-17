import type { EnvGroup } from "./configTypes";

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
    id: "urls",
    label: "Data & Service URLs",
    icon: "server",
    note: "Endpoint URLs and the config home. Live in .env, never config.toml.",
    vars: [
      {
        key: "AXON_DATA_DIR",
        type: "text",
        def: "~/.axon",
        desc: "Config home: .env, config.toml, jobs.db, output, logs, qdrant, tei.",
      },
      {
        key: "AXON_HOME",
        type: "text",
        def: "~/.axon",
        desc: "Axon home root for Docker bind mounts.",
      },
      {
        key: "QDRANT_URL",
        type: "text",
        def: "http://127.0.0.1:53333",
        desc: "Qdrant vector store endpoint.",
      },
      {
        key: "TEI_URL",
        type: "text",
        def: "http://127.0.0.1:52000",
        desc: "Hugging Face TEI embeddings endpoint (Qwen3).",
      },
      {
        key: "AXON_CHROME_REMOTE_URL",
        type: "text",
        def: "http://127.0.0.1:6000",
        desc: "Chrome render / CDP proxy endpoint.",
      },
      {
        key: "AXON_COLLECTION",
        type: "text",
        def: "axon",
        desc: "Default Qdrant collection name.",
      },
    ],
  },
  {
    id: "server-auth",
    label: "Server & Auth",
    icon: "shield",
    note: "Unified HTTP server and auth. OAuth fields apply only when auth mode is oauth.",
    vars: [
      {
        key: "AXON_HTTP_TOKEN",
        type: "secret",
        def: "",
        desc: "Static bearer token for unified HTTP surfaces.",
      },
      {
        key: "AXON_AUTH_MODE",
        type: "enum",
        options: ["bearer", "oauth"],
        def: "bearer",
        desc: "Auth policy for HTTP and MCP routes.",
      },
      {
        key: "AXON_PUBLIC_URL",
        type: "text",
        def: "",
        desc: "Public base URL advertised to OAuth clients.",
      },
      { key: "AXON_GOOGLE_CLIENT_ID", type: "text", def: "", desc: "Google OAuth client id." },
      {
        key: "AXON_GOOGLE_CLIENT_SECRET",
        type: "secret",
        def: "",
        desc: "Google OAuth client secret.",
      },
      {
        key: "AXON_AUTH_ADMIN_EMAIL",
        type: "text",
        def: "",
        desc: "Admin email granted full server scope under OAuth.",
      },
      {
        key: "AXON_ALLOWED_REDIRECT_URIS",
        type: "text",
        def: "",
        desc: "Comma-separated allowed OAuth redirect URIs.",
      },
      {
        key: "AXON_ALLOWED_ORIGINS",
        type: "text",
        def: "",
        desc: "Comma-separated allowed CORS origins.",
      },
    ],
  },
  {
    id: "source",
    label: "Source Credentials",
    icon: "key",
    note: "Optional API keys. Each unlocks a source or higher rate limits.",
    vars: [
      { key: "TAVILY_API_KEY", type: "secret", def: "", desc: "Tavily key for search / research." },
      {
        key: "AXON_SEARXNG_URL",
        type: "text",
        def: "",
        desc: "Self-hosted SearXNG (JSON enabled). Overrides Tavily for research.",
      },
      {
        key: "GITHUB_TOKEN",
        type: "secret",
        def: "",
        desc: "Higher-rate GitHub source indexing (code, issues, PRs, wiki).",
      },
      { key: "GITLAB_TOKEN", type: "secret", def: "", desc: "GitLab repository source indexing." },
      { key: "GITEA_TOKEN", type: "secret", def: "", desc: "Gitea repository source indexing." },
      {
        key: "REDDIT_CLIENT_ID",
        type: "secret",
        def: "",
        desc: "Reddit app client id for subreddit source indexing.",
      },
      { key: "REDDIT_CLIENT_SECRET", type: "secret", def: "", desc: "Reddit app client secret." },
      {
        key: "HF_TOKEN",
        type: "secret",
        def: "",
        desc: "Hugging Face token for gated TEI model pulls.",
      },
    ],
  },
  {
    id: "llm",
    label: "LLM Runtime",
    icon: "brain",
    note: "Synthesis backend for ask / evaluate / suggest / extract / research. Default: Gemini headless.",
    vars: [
      {
        key: "AXON_LLM_BACKEND",
        type: "enum",
        options: ["gemini", "openai-compat"],
        def: "gemini",
        desc: "Synthesis backend. Empty = Gemini headless.",
      },
      {
        key: "AXON_OPENAI_BASE_URL",
        type: "text",
        def: "",
        desc: "OpenAI-compatible API root (llama.cpp, LM Studio). No /chat/completions.",
      },
      {
        key: "AXON_SYNTHESIS_OPENAI_MODEL",
        type: "text",
        def: "",
        desc: "Model used for synthesized answers with the OpenAI-compatible backend.",
      },
      {
        key: "AXON_CHAT_OPENAI_MODEL",
        type: "text",
        def: "",
        desc: "Model used for direct chat with the OpenAI-compatible backend.",
      },
      {
        key: "AXON_OPENAI_API_KEY",
        type: "secret",
        def: "",
        desc: "Optional key for the OpenAI-compatible endpoint.",
      },
      {
        key: "GEMINI_API_KEY",
        type: "secret",
        def: "",
        desc: "Gemini API key. Leave blank to use OAuth under $HOME/.gemini.",
      },
      {
        key: "GEMINI_HOME",
        type: "text",
        def: "",
        desc: "Host dir holding Gemini CLI OAuth credentials (Docker mount).",
      },
      {
        key: "AXON_HEADLESS_GEMINI_HOME",
        type: "text",
        def: "$HOME",
        desc: "Dir Axon copies OAuth files FROM per invocation.",
      },
      {
        key: "AXON_HEADLESS_GEMINI_CMD",
        type: "text",
        def: "gemini",
        desc: "Path to the gemini binary.",
      },
      {
        key: "AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL",
        type: "text",
        def: "",
        desc: "Model used for synthesized answers with Gemini headless.",
      },
      {
        key: "AXON_CHAT_HEADLESS_GEMINI_MODEL",
        type: "text",
        def: "",
        desc: "Model used for direct chat with Gemini headless.",
      },
    ],
  },
  {
    id: "http",
    label: "HTTP Behavior",
    icon: "globe",
    note: "User-Agent strings for all outbound HTTP and Chrome render paths.",
    vars: [
      {
        key: "AXON_USER_AGENT",
        type: "text",
        def: "",
        desc: "UA for all HTTP requests. Falls back to a Firefox UA.",
      },
      {
        key: "AXON_CHROME_USER_AGENT",
        type: "text",
        def: "",
        desc: "Chrome-specific UA. Falls back to AXON_USER_AGENT.",
      },
    ],
  },
  {
    id: "logging",
    label: "Logging",
    icon: "file",
    note: "Log rotation is env-only — init_tracing() runs before config.toml is read.",
    vars: [
      {
        key: "AXON_LOG_PATH",
        type: "text",
        def: "$AXON_DATA_DIR/logs/axon.log",
        desc: "Active log file. Rotated siblings (.1, .2…) live alongside.",
      },
      {
        key: "AXON_LOG_MAX_BYTES",
        type: "int",
        def: "10485760",
        desc: "Rotation threshold in bytes (10 MiB). 0 disables rotation.",
      },
    ],
  },
  {
    id: "docker",
    label: "Docker / Compose",
    icon: "layers",
    note: "Compose interpolation and TEI/GPU bootstrap. Docker path only.",
    vars: [
      { key: "AXON_IMAGE", type: "text", def: "", desc: "Axon server image tag for Compose." },
      {
        key: "AXON_HTTP_PUBLISH",
        type: "text",
        def: "8001",
        desc: "Published host address for the unified HTTP server.",
      },
      {
        key: "TEI_EMBEDDING_MODEL",
        type: "text",
        def: "Qwen/Qwen3-Embedding-0.6B",
        desc: "Production embedding model served by TEI.",
      },
      { key: "TEI_HTTP_PORT", type: "int", def: "52000", desc: "TEI host port." },
      {
        key: "TEI_SERVER_MAX_CLIENT_BATCH_SIZE",
        type: "int",
        def: "96",
        desc: "TEI server-side max client batch size.",
      },
      {
        key: "NVIDIA_VISIBLE_DEVICES",
        type: "text",
        def: "0",
        desc: "GPU device(s) exposed to the TEI container.",
      },
      { key: "CUDA_VISIBLE_DEVICES", type: "text", def: "0", desc: "CUDA device ordinal." },
    ],
  },
];
