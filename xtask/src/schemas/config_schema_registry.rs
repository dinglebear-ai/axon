//! Config + env registry data backing the `config` schema family generator.
//!
//! This module projects the live `config.toml` tuning contract and `.env`
//! bootstrap/secret contract into generated schema artifacts. Keep it aligned
//! with `config.example.toml`, `docs/guides/configuration.md`, and the parser in
//! `crates/axon-core/src/config`.

/// One `config.toml` setting from the contract's "Required Config Keys" table.
pub struct ConfigKeySpec {
    pub key: &'static str,
    pub section: &'static str,
    pub kind: &'static str,
    pub default_json: &'static str,
    pub owner_crate: &'static str,
    pub env_override: Option<&'static str>,
    /// Whether this key holds secret material. Per the live configuration rule
    /// ("Secrets and deployment URLs stay in `.env`"), every key in this
    /// registry is non-secret by construction — a secret-shaped tuning knob
    /// belongs in the env var registry instead, not here.
    pub secret: bool,
    /// Whether changing this key requires a process restart to take effect.
    /// Axon has no config hot-reload path today (config is loaded once at
    /// process start — see `crates/axon-core/src/config`), so `true` is the
    /// accurate value for every current key.
    pub restart_required: bool,
    pub description: &'static str,
}

/// One `.env` variable from the contract's "Required Env Variables" table.
pub struct EnvVarSpec {
    pub name: &'static str,
    pub required: bool,
    pub secret: bool,
    pub default: Option<&'static str>,
    pub owner_crate: &'static str,
    pub compose_usage: bool,
    pub validation: &'static str,
    pub example_allowed: bool,
    pub description: &'static str,
}

type RawConfigKey = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
    bool,
    bool,
    &'static str,
);

// (key, section, kind, default_json, owner_crate, env_key, secret, restart_required, description)
//
// `secret` is `false` for every row by construction — see `ConfigKeySpec::secret`'s
// doc comment; a secret-shaped key belongs in the env var registry, not here.
// `restart_required` is `true` because config is loaded once at process start.
// `env_key` is populated only where a live override name is documented in
// `config.example.toml`, `docs/guides/configuration.md`, or the root `CLAUDE.md`
// reference and is not itself a token `registry.rs::REMOVED_SURFACE_RULES`
// bans from ever reappearing in generated `docs/reference/config/` output —
// `AXON_COLLECTION`, `AXON_HYBRID_CANDIDATES`, `AXON_ASK_HYBRID_CANDIDATES`,
// `AXON_WATCH_TICK_SECS`, and `AXON_WATCH_LEASE_SECS` are legacy env-override
// names retired in favor of TOML-only keys, so those five stay `None` here
// (`config_keys_and_env_vars_do_not_reuse_removed_names` enforces this).
// Every other key without a documented override is `None` rather than
// guessed.
const RAW_CONFIG_KEYS: &[RawConfigKey] = &[
    (
        "server.default_collection",
        "server",
        "string",
        "\"axon\"",
        "axon-web",
        // NOT `AXON_COLLECTION` — that name is in the removed-surface
        // registry (`registry.rs::REMOVED_SURFACE_RULES`); this key is
        // TOML-only.
        None,
        false,
        true,
        "Default vector collection.",
    ),
    (
        "server.json_pretty",
        "server",
        "boolean",
        "false",
        "axon-web",
        None,
        false,
        true,
        "Pretty JSON for CLI/API when requested.",
    ),
    (
        "pipeline.max_active_source_jobs",
        "pipeline",
        "integer",
        "4",
        "axon-services",
        None,
        false,
        true,
        "Concurrent source jobs.",
    ),
    (
        "pipeline.max_active_interactive_jobs",
        "pipeline",
        "integer",
        "8",
        "axon-services",
        None,
        false,
        true,
        "Concurrent ask/query/retrieve jobs.",
    ),
    (
        "jobs.heartbeat_secs",
        "jobs",
        "integer",
        "15",
        "axon-jobs",
        None,
        false,
        true,
        "Active job heartbeat interval.",
    ),
    (
        "jobs.provider_reservation_timeout_secs",
        "jobs",
        "integer",
        "30",
        "axon-jobs",
        None,
        false,
        true,
        "Provider reservation timeout.",
    ),
    (
        "sources.embed_by_default",
        "sources",
        "boolean",
        "true",
        "axon-services",
        None,
        false,
        true,
        "Source jobs write vectors unless --no-embed.",
    ),
    (
        "sources.default_scope_web",
        "sources",
        "enum",
        "\"site\"",
        "axon-services",
        None,
        false,
        true,
        "Default web scope.",
    ),
    (
        "sources.default_scope_local",
        "sources",
        "enum",
        "\"directory\"",
        "axon-services",
        None,
        false,
        true,
        "Default local path scope.",
    ),
    (
        "watch.tick_secs",
        "watch",
        "integer",
        "15",
        "axon-jobs",
        // NOT `AXON_WATCH_TICK_SECS` — removed-surface registry entry; see
        // the RAW_CONFIG_KEYS doc comment above.
        None,
        false,
        true,
        "Watch scheduler sweep interval.",
    ),
    (
        "watch.lease_secs",
        "watch",
        "integer",
        "300",
        "axon-jobs",
        // NOT `AXON_WATCH_LEASE_SECS` — removed-surface registry entry; see
        // the RAW_CONFIG_KEYS doc comment above.
        None,
        false,
        true,
        "Watch lease TTL.",
    ),
    (
        "providers.embedding.batch_size",
        "providers",
        "integer",
        "128",
        "axon-embedding",
        Some("AXON_EMBEDDING_BATCH_SIZE"),
        false,
        true,
        "Maximum chunks per embedding request.",
    ),
    (
        "providers.embedding.max_concurrent_requests",
        "providers",
        "integer",
        "4",
        "axon-embedding",
        None,
        false,
        true,
        "Max concurrent embedding requests.",
    ),
    (
        "providers.embedding.interactive_reserved_requests",
        "providers",
        "integer",
        "1",
        "axon-jobs",
        None,
        false,
        true,
        "Requests reserved for ask/query embeddings.",
    ),
    (
        "providers.vector.write_concurrency",
        "providers",
        "integer",
        "4",
        "axon-vectors",
        None,
        false,
        true,
        "Concurrent vector writes.",
    ),
    (
        "providers.vector.read_concurrency",
        "providers",
        "integer",
        "16",
        "axon-vectors",
        None,
        false,
        true,
        "Concurrent vector reads.",
    ),
    (
        "providers.llm.completion_concurrency",
        "providers",
        "integer",
        "4",
        "axon-llm",
        Some("AXON_LLM_COMPLETION_CONCURRENCY"),
        false,
        true,
        "Global LLM completions.",
    ),
    (
        "providers.search.default",
        "providers",
        "enum",
        "\"searxng-then-tavily\"",
        "axon-adapters",
        None,
        false,
        true,
        "Default search backend order.",
    ),
    (
        "retrieval.limit",
        "retrieval",
        "integer",
        "10",
        "axon-retrieval",
        None,
        false,
        true,
        "Default query result count.",
    ),
    (
        "retrieval.hybrid_candidates",
        "retrieval",
        "integer",
        "100",
        "axon-retrieval",
        // NOT `AXON_HYBRID_CANDIDATES` — removed-surface registry entry; see
        // the RAW_CONFIG_KEYS doc comment above.
        None,
        false,
        true,
        "RRF prefetch per arm.",
    ),
    (
        "retrieval.ask_hybrid_candidates",
        "retrieval",
        "integer",
        "150",
        "axon-retrieval",
        // NOT `AXON_ASK_HYBRID_CANDIDATES` — removed-surface registry entry;
        // see the RAW_CONFIG_KEYS doc comment above.
        None,
        false,
        true,
        "Wider ask retrieval prefetch.",
    ),
    (
        "crawl.max_pages",
        "crawl",
        "integer",
        "2000",
        "axon-adapters",
        None,
        false,
        true,
        "Default site page cap.",
    ),
    (
        "crawl.respect_robots",
        "crawl",
        "boolean",
        "false",
        "axon-adapters",
        None,
        false,
        true,
        "Respect robots.txt directives.",
    ),
    (
        "memory.decay_enabled",
        "memory",
        "boolean",
        "true",
        "axon-memory",
        None,
        false,
        true,
        "Enable memory decay scoring.",
    ),
    (
        "memory.review_interval_days",
        "memory",
        "integer",
        "30",
        "axon-memory",
        None,
        false,
        true,
        "Memory review cadence.",
    ),
    (
        "graph.enabled",
        "graph",
        "boolean",
        "true",
        "axon-graph",
        None,
        false,
        true,
        "Enable graph candidate ingestion.",
    ),
    (
        "prune.retention_days.jobs",
        "prune",
        "integer",
        "14",
        "axon-prune",
        None,
        false,
        true,
        "Job event retention before prune.",
    ),
    (
        "observability.log_level",
        "observability",
        "enum",
        "\"info\"",
        "axon-observe",
        None,
        false,
        true,
        "Default Axon log level.",
    ),
    (
        "security.allow_private_network_fetch",
        "security",
        "boolean",
        "false",
        "axon-authz",
        None,
        false,
        true,
        "SSRF private IP allowance.",
    ),
];

pub fn config_key_registry() -> Vec<ConfigKeySpec> {
    RAW_CONFIG_KEYS
        .iter()
        .map(
            |&(
                key,
                section,
                kind,
                default_json,
                owner_crate,
                env_override,
                secret,
                restart_required,
                description,
            )| {
                debug_assert!(
                    REQUIRED_CONFIG_SECTIONS.contains(&section),
                    "config key {key} has section {section} outside the 20-section contract"
                );
                ConfigKeySpec {
                    key,
                    section,
                    kind,
                    default_json,
                    owner_crate,
                    env_override,
                    secret,
                    restart_required,
                    description,
                }
            },
        )
        .collect()
}

/// The 15 required top-level `config.toml` sections from the contract.
pub const REQUIRED_CONFIG_SECTIONS: &[&str] = &[
    "server",
    "sources",
    "pipeline",
    "watch",
    "jobs",
    "providers",
    "retrieval",
    "ask",
    "crawl",
    "memory",
    "graph",
    "artifacts",
    "prune",
    "observability",
    "security",
];

#[path = "config_schema_registry/env_vars.rs"]
mod env_vars;
pub use env_vars::env_var_registry;

#[cfg(test)]
#[path = "config_schema_registry_tests.rs"]
mod tests;
