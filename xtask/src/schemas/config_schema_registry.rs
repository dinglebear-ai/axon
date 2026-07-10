//! Config + env registry data backing the `config` schema family generator.
//!
//! This module is the xtask-local source of truth for the settled 20-section
//! `config.toml` contract and the `.env` bootstrap/secret contract. It is
//! intentionally independent of `axon-core`'s runtime config (which has not
//! yet implemented this shape) so the generator can reflect the contract
//! ahead of the runtime cutover. See:
//! - `docs/pipeline-unification/schemas/config-schema.md` (artifact shape)
//! - `docs/pipeline-unification/configuration/config-contract.md` (20-section
//!   shape + required key table)

/// One `config.toml` setting from the contract's "Required Config Keys" table.
pub struct ConfigKeySpec {
    pub key: &'static str,
    pub section: &'static str,
    pub kind: &'static str,
    pub default_json: &'static str,
    pub owner_crate: &'static str,
    pub env_override: Option<&'static str>,
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
    &'static str,
);

// (key, section, kind, default_json, owner_crate, description)
const RAW_CONFIG_KEYS: &[RawConfigKey] = &[
    (
        "server.default_collection",
        "server",
        "string",
        "\"axon\"",
        "axon-web",
        "Default vector collection.",
    ),
    (
        "server.json_pretty",
        "server",
        "boolean",
        "false",
        "axon-web",
        "Pretty JSON for CLI/API when requested.",
    ),
    (
        "pipeline.max_active_source_jobs",
        "pipeline",
        "integer",
        "4",
        "axon-services",
        "Concurrent source jobs.",
    ),
    (
        "pipeline.max_active_interactive_jobs",
        "pipeline",
        "integer",
        "8",
        "axon-services",
        "Concurrent ask/query/retrieve jobs.",
    ),
    (
        "jobs.heartbeat_secs",
        "jobs",
        "integer",
        "15",
        "axon-jobs",
        "Active job heartbeat interval.",
    ),
    (
        "jobs.provider_reservation_timeout_secs",
        "jobs",
        "integer",
        "30",
        "axon-jobs",
        "Provider reservation timeout.",
    ),
    (
        "sources.embed_by_default",
        "sources",
        "boolean",
        "true",
        "axon-services",
        "Source jobs write vectors unless --no-embed.",
    ),
    (
        "sources.default_scope_web",
        "sources",
        "enum",
        "\"site\"",
        "axon-services",
        "Default web scope.",
    ),
    (
        "sources.default_scope_local",
        "sources",
        "enum",
        "\"directory\"",
        "axon-services",
        "Default local path scope.",
    ),
    (
        "watch.tick_secs",
        "watch",
        "integer",
        "15",
        "axon-jobs",
        "Watch scheduler sweep interval.",
    ),
    (
        "watch.lease_secs",
        "watch",
        "integer",
        "300",
        "axon-jobs",
        "Watch lease TTL.",
    ),
    (
        "providers.embedding.batch_size",
        "providers",
        "integer",
        "128",
        "axon-embedding",
        "Maximum chunks per embedding request.",
    ),
    (
        "providers.embedding.max_concurrent_requests",
        "providers",
        "integer",
        "4",
        "axon-embedding",
        "Max concurrent embedding requests.",
    ),
    (
        "providers.embedding.interactive_reserved_requests",
        "providers",
        "integer",
        "1",
        "axon-jobs",
        "Requests reserved for ask/query embeddings.",
    ),
    (
        "providers.vector.write_concurrency",
        "providers",
        "integer",
        "4",
        "axon-vectors",
        "Concurrent vector writes.",
    ),
    (
        "providers.vector.read_concurrency",
        "providers",
        "integer",
        "16",
        "axon-vectors",
        "Concurrent vector reads.",
    ),
    (
        "providers.llm.completion_concurrency",
        "providers",
        "integer",
        "4",
        "axon-llm",
        "Global LLM completions.",
    ),
    (
        "providers.search.default",
        "providers",
        "enum",
        "\"searxng-then-tavily\"",
        "axon-adapters",
        "Default search backend order.",
    ),
    (
        "retrieval.limit",
        "retrieval",
        "integer",
        "10",
        "axon-retrieval",
        "Default query result count.",
    ),
    (
        "retrieval.hybrid_candidates",
        "retrieval",
        "integer",
        "100",
        "axon-retrieval",
        "RRF prefetch per arm.",
    ),
    (
        "retrieval.ask_hybrid_candidates",
        "retrieval",
        "integer",
        "150",
        "axon-retrieval",
        "Wider ask retrieval prefetch.",
    ),
    (
        "crawl.max_pages",
        "crawl",
        "integer",
        "2000",
        "axon-adapters",
        "Default site page cap.",
    ),
    (
        "crawl.respect_robots",
        "crawl",
        "boolean",
        "false",
        "axon-adapters",
        "Respect robots.txt directives.",
    ),
    (
        "memory.decay_enabled",
        "memory",
        "boolean",
        "true",
        "axon-memory",
        "Enable memory decay scoring.",
    ),
    (
        "memory.review_interval_days",
        "memory",
        "integer",
        "30",
        "axon-memory",
        "Memory review cadence.",
    ),
    (
        "graph.enabled",
        "graph",
        "boolean",
        "true",
        "axon-graph",
        "Enable graph candidate ingestion.",
    ),
    (
        "prune.retention_days.jobs",
        "prune",
        "integer",
        "14",
        "axon-prune",
        "Job event retention before prune.",
    ),
    (
        "observability.log_level",
        "observability",
        "enum",
        "\"info\"",
        "axon-observe",
        "Default Axon log level.",
    ),
    (
        "security.allow_private_network_fetch",
        "security",
        "boolean",
        "false",
        "axon-authz",
        "SSRF private IP allowance.",
    ),
];

pub fn config_key_registry() -> Vec<ConfigKeySpec> {
    RAW_CONFIG_KEYS
        .iter()
        .map(
            |&(key, section, kind, default_json, owner_crate, description)| {
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
                    env_override: None,
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
