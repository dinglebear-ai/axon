//! Runtime configuration registry used by schema-contract generation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigKeySpec {
    pub key: &'static str,
    pub section: &'static str,
    pub env_key: Option<&'static str>,
    pub secret: bool,
}

pub fn config_key_registry() -> &'static [ConfigKeySpec] {
    &[
        ConfigKeySpec {
            key: "server.bind",
            section: "server",
            env_key: Some("AXON_BIND"),
            secret: false,
        },
        ConfigKeySpec {
            key: "sources.default_collection",
            section: "sources",
            env_key: None,
            secret: false,
        },
        ConfigKeySpec {
            key: "pipeline.max_pages",
            section: "pipeline",
            env_key: Some("AXON_MAX_PAGES"),
            secret: false,
        },
        ConfigKeySpec {
            key: "watch.tick_secs",
            section: "watch",
            env_key: None,
            secret: false,
        },
        ConfigKeySpec {
            key: "jobs.stale_timeout_secs",
            section: "jobs",
            env_key: Some("AXON_JOB_STALE_TIMEOUT_SECS"),
            secret: false,
        },
        ConfigKeySpec {
            key: "providers.tei_url",
            section: "providers",
            env_key: Some("TEI_URL"),
            secret: false,
        },
        ConfigKeySpec {
            key: "retrieval.hybrid_candidates",
            section: "retrieval",
            env_key: None,
            secret: false,
        },
        ConfigKeySpec {
            key: "memory.enabled",
            section: "memory",
            env_key: Some("AXON_MEMORY_ENABLED"),
            secret: false,
        },
        ConfigKeySpec {
            key: "graph.enabled",
            section: "graph",
            env_key: Some("AXON_GRAPH_ENABLED"),
            secret: false,
        },
        ConfigKeySpec {
            key: "observability.log_level",
            section: "observability",
            env_key: Some("RUST_LOG"),
            secret: false,
        },
        ConfigKeySpec {
            key: "auth.allowed_email",
            section: "auth",
            env_key: Some("AXON_AUTH_ALLOWED_EMAIL"),
            secret: false,
        },
    ]
}

pub fn removed_env_keys() -> &'static [&'static str] {
    &[
        "AXON_MCP_HTTP_TOKEN",
        "AXON_MCP_AUTH_MODE",
        "AXON_MCP_GOOGLE_CLIENT_SECRET",
        "AXON_MCP_ALLOWED_ORIGINS",
    ]
}
