package com.axon.app.feature.settings

internal val axonEnvSettingGroups =
    listOf(
        SettingGroup(
            id = "urls",
            label = "Data & Service URLs",
            icon = "server",
            note = "Endpoint URLs and the config home. Live in .env, never config.toml.",
            fields =
                listOf(
                    SettingField(
                        "AXON_DATA_DIR",
                        SettingKind.Text,
                        "",
                        "Config home: .env, config.toml, jobs.db, output, logs, qdrant, tei.",
                    ),
                    SettingField("AXON_HOME", SettingKind.Text, "", "Axon home root for Docker bind mounts."),
                    SettingField("QDRANT_URL", SettingKind.Text, "http://127.0.0.1:53333", "Qdrant vector store endpoint."),
                    SettingField("TEI_URL", SettingKind.Text, "http://127.0.0.1:52000", "Hugging Face TEI embeddings endpoint."),
                    SettingField(
                        "AXON_CHROME_REMOTE_URL",
                        SettingKind.Text,
                        "http://127.0.0.1:6000",
                        "Chrome render / CDP proxy endpoint.",
                    ),
                    SettingField("AXON_COLLECTION", SettingKind.Text, "axon", "Default Qdrant collection name."),
                ),
        ),
        SettingGroup(
            id = "server-auth",
            label = "Server & Auth",
            icon = "shield",
            note = "Unified HTTP server and auth. OAuth fields apply only when auth mode is oauth.",
            fields =
                listOf(
                    SettingField("AXON_HTTP_TOKEN", SettingKind.Secret, "", "Static bearer token for unified HTTP surfaces."),
                    SettingField(
                        "AXON_AUTH_MODE",
                        SettingKind.Enum,
                        "bearer",
                        "Auth policy for HTTP and MCP routes.",
                        options = listOf("bearer", "oauth"),
                    ),
                    SettingField("AXON_PUBLIC_URL", SettingKind.Text, "", "Public base URL advertised to OAuth clients."),
                    SettingField("AXON_GOOGLE_CLIENT_ID", SettingKind.Text, "", "Google OAuth client id."),
                    SettingField("AXON_GOOGLE_CLIENT_SECRET", SettingKind.Secret, "", "Google OAuth client secret."),
                    SettingField("AXON_AUTH_ADMIN_EMAIL", SettingKind.Text, "", "Admin email granted full server scope under OAuth."),
                    SettingField("AXON_ALLOWED_REDIRECT_URIS", SettingKind.Text, "", "Comma-separated allowed OAuth redirect URIs."),
                    SettingField("AXON_ALLOWED_ORIGINS", SettingKind.Text, "", "Comma-separated allowed CORS origins."),
                ),
        ),
        SettingGroup(
            id = "source-credentials",
            label = "Source Credentials",
            icon = "key",
            note = "Optional API keys. Each unlocks a source or higher rate limits.",
            fields =
                listOf(
                    SettingField("TAVILY_API_KEY", SettingKind.Secret, "", "Tavily key for search / research."),
                    SettingField(
                        "AXON_SEARXNG_URL",
                        SettingKind.Text,
                        "",
                        "Self-hosted SearXNG search endpoint. Overrides Tavily for research.",
                    ),
                    SettingField("GITHUB_TOKEN", SettingKind.Secret, "", "Higher-rate GitHub source indexing."),
                    SettingField("GITLAB_TOKEN", SettingKind.Secret, "", "GitLab repository source indexing."),
                    SettingField("GITEA_TOKEN", SettingKind.Secret, "", "Gitea repository source indexing."),
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
            fields =
                listOf(
                    SettingField(
                        "AXON_LLM_BACKEND",
                        SettingKind.Enum,
                        "",
                        "Synthesis backend. Empty = Gemini headless.",
                        options = listOf("", "gemini", "openai-compat"),
                    ),
                    SettingField("AXON_OPENAI_BASE_URL", SettingKind.Text, "", "OpenAI-compatible API root. No /chat/completions."),
                    SettingField("AXON_SYNTHESIS_OPENAI_MODEL", SettingKind.Text, "", "OpenAI-compatible model for RAG synthesis."),
                    SettingField(
                        "AXON_CHAT_OPENAI_MODEL",
                        SettingKind.Text,
                        "",
                        "OpenAI-compatible model for direct Chat mode. Empty uses synthesis model.",
                    ),
                    SettingField("AXON_OPENAI_API_KEY", SettingKind.Secret, "", "Optional key for the OpenAI-compatible endpoint."),
                    SettingField(
                        "GEMINI_API_KEY",
                        SettingKind.Secret,
                        "",
                        "Gemini API key. Leave blank to use OAuth under HOME/.gemini.",
                    ),
                    SettingField("GEMINI_HOME", SettingKind.Text, "", "Host dir holding Gemini CLI OAuth credentials."),
                    SettingField("AXON_HEADLESS_GEMINI_HOME", SettingKind.Text, "", "Dir Axon copies OAuth files FROM per invocation."),
                    SettingField("AXON_HEADLESS_GEMINI_CMD", SettingKind.Text, "", "Path to the gemini binary."),
                    SettingField("AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL", SettingKind.Text, "", "Gemini model for RAG synthesis."),
                    SettingField(
                        "AXON_CHAT_HEADLESS_GEMINI_MODEL",
                        SettingKind.Text,
                        "",
                        "Gemini model for direct Chat mode. Empty uses synthesis model.",
                    ),
                ),
        ),
        SettingGroup(
            id = "http",
            label = "HTTP Behavior",
            icon = "globe",
            note = "User-Agent strings for outbound HTTP and Chrome render paths.",
            fields =
                listOf(
                    SettingField("AXON_USER_AGENT", SettingKind.Text, "", "UA for all HTTP requests."),
                    SettingField("AXON_CHROME_USER_AGENT", SettingKind.Text, "", "Chrome-specific UA override."),
                ),
        ),
        SettingGroup(
            id = "logging",
            label = "Logging",
            icon = "file",
            note = "Log rotation is env-only because tracing starts before config.toml is read.",
            fields =
                listOf(
                    SettingField("AXON_LOG_PATH", SettingKind.Text, "", "Active log file path."),
                ),
        ),
        SettingGroup(
            id = "docker",
            label = "Docker / Compose",
            icon = "layers",
            note = "Compose interpolation and TEI/GPU bootstrap.",
            fields =
                listOf(
                    SettingField("AXON_IMAGE", SettingKind.Text, "", "Axon server image tag for Compose."),
                    SettingField("AXON_HTTP_PUBLISH", SettingKind.Text, "8001", "Published host address for the unified HTTP server."),
                    SettingField(
                        "TEI_EMBEDDING_MODEL",
                        SettingKind.Text,
                        "Qwen/Qwen3-Embedding-0.6B",
                        "Production embedding model served by TEI.",
                    ),
                    SettingField("TEI_HTTP_PORT", SettingKind.Int, "52000", "TEI host port."),
                    SettingField("TEI_SERVER_MAX_CLIENT_BATCH_SIZE", SettingKind.Int, "96", "TEI server-side max client batch size."),
                    SettingField("NVIDIA_VISIBLE_DEVICES", SettingKind.Text, "0", "GPU devices exposed to TEI."),
                    SettingField("CUDA_VISIBLE_DEVICES", SettingKind.Text, "0", "CUDA device ordinal."),
                ),
        ),
    )
