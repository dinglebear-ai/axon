//! Config + env registry data backing the `config` schema family generator.
//!
//! This module is the xtask-local source of truth for the settled 20-section
//! `config.toml` contract and the `.env` bootstrap/secret contract. It is
//! intentionally separate from `axon-core`'s runtime config structs, but its
//! keys, defaults, and enforcement descriptions must match the shipped parser
//! and provider behavior. See:
//! - `docs/pipeline-unification/schemas/config-schema.md` (artifact shape)
//! - `docs/pipeline-unification/configuration/config-contract.md` (20-section
//!   shape + required key table)

#[path = "config_schema_registry/types.rs"]
mod types;
use types::RawConfigKey;
pub use types::{ConfigKeySpec, EnvVarSpec};

// (key, section, kind, default_json, owner_crate, env_key, secret, restart_required, description)
//
// `secret` is `false` for every row by construction — see `ConfigKeySpec::secret`'s
// doc comment; a secret-shaped key belongs in the env var registry, not here.
// `restart_required` is `true` for current runtime-backed keys because config
// is loaded once at process start and Axon has no hot-reload path today.
// `env_key` is populated only where a target or currently-shipped override name
// is documented (`docs/pipeline-unification/schemas/config-schema.md`'s worked
// example, or the currently-shipped env vars in the root `CLAUDE.md` env
// reference) AND is not itself a token `registry.rs::REMOVED_SURFACE_RULES`
// bans from ever reappearing in generated `docs/reference/config/` output —
// `AXON_COLLECTION`, `AXON_HYBRID_CANDIDATES`, `AXON_ASK_HYBRID_CANDIDATES`,
// `AXON_WATCH_TICK_SECS`, and `AXON_WATCH_LEASE_SECS` are legacy env-override
// names being fully retired in favor of TOML-only keys, so those five stay
// `None` here even though they're real, currently-shipped overrides today
// (`config_keys_and_env_vars_do_not_reuse_removed_names` enforces this).
// Every other key without a documented override is `None` rather than
// guessed.
pub(super) const RAW_CONFIG_KEYS: &[RawConfigKey] = &[
    (
        "server.default_collection",
        "server",
        "string",
        "\"axon\"",
        "axon-web",
        // NOT `AXON_COLLECTION` — that name is in the removed-surface
        // registry (`registry.rs::REMOVED_SURFACE_RULES`); the target design
        // is TOML-only for this key going forward.
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
        "96",
        "axon-embedding",
        Some("TEI_MAX_CLIENT_BATCH_SIZE"),
        false,
        true,
        "Maximum chunks per embedding request.",
    ),
    (
        "providers.embedding.max_concurrent_requests",
        "providers",
        "integer",
        "8",
        "axon-embedding",
        Some("AXON_TEI_MAX_CONCURRENT"),
        false,
        true,
        "Process-shared concurrent TEI requests for one endpoint/admission profile.",
    ),
    (
        "providers.embedding.max_in_flight_inputs",
        "providers",
        "integer",
        "320",
        "axon-embedding",
        Some("AXON_TEI_MAX_IN_FLIGHT_INPUTS"),
        false,
        true,
        "Process-shared aggregate TEI inputs across concurrent requests.",
    ),
    (
        "providers.embedding.interactive_reserved_requests",
        "providers",
        "integer",
        "1",
        "axon-jobs",
        Some("AXON_TEI_INTERACTIVE_RESERVED_REQUESTS"),
        false,
        true,
        "Requests reserved for ask/query embeddings.",
    ),
    (
        "providers.embedding.cache_enabled",
        "providers",
        "boolean",
        "false",
        "axon-services",
        Some("AXON_EMBED_CACHE_ENABLED"),
        false,
        true,
        "Persist dense vectors for warm reuse; disabled by default to avoid cold-ingestion latency.",
    ),
    (
        "providers.embedding.cache_max_entries",
        "providers",
        "integer",
        "100000",
        "axon-services",
        Some("AXON_EMBED_CACHE_MAX_ENTRIES"),
        false,
        true,
        "Maximum vectors retained by the persistent embedding cache.",
    ),
    (
        "providers.embedding.scheduler_enabled",
        "providers",
        "boolean",
        "true",
        "axon-services",
        Some("AXON_EMBED_SCHEDULER_ENABLED"),
        false,
        true,
        "Use the bounded preparation and embedding scheduler.",
    ),
    (
        "providers.embedding.max_batch_tokens",
        "providers",
        "integer",
        "65536",
        "axon-embedding",
        Some("AXON_TEI_CLIENT_MAX_BATCH_TOKENS"),
        false,
        true,
        "Conservative maximum token budget for one TEI client batch.",
    ),
    (
        "providers.embedding.vector_upsert_overlap_enabled",
        "providers",
        "boolean",
        "true",
        "axon-services",
        Some("AXON_VECTOR_UPSERT_EMBED_OVERLAP"),
        false,
        true,
        "Overlap the next embedding request with the current vector upsert.",
    ),
    (
        "providers.vector.write_concurrency",
        "providers",
        "integer",
        "1",
        "axon-vectors",
        None,
        false,
        true,
        "Process-shared concurrent vector point/generation writes.",
    ),
    (
        "providers.vector.upsert_batch_points",
        "providers",
        "integer",
        "1024",
        "axon-vectors",
        Some("AXON_QDRANT_UPSERT_BATCH_SIZE"),
        false,
        true,
        "Maximum points per Qdrant upsert request.",
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
        "Accepted for forward compatibility; not currently enforced.",
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

#[path = "config_schema_registry/projection_batch.rs"]
mod projection_batch;
pub(super) use projection_batch::PROJECTION_BATCH_KEYS;

#[path = "config_schema_registry/keys.rs"]
mod keys;
#[cfg(test)]
pub use keys::REQUIRED_CONFIG_SECTIONS;
pub use keys::config_key_registry;

#[path = "config_schema_registry/env_vars.rs"]
mod env_vars;
pub use env_vars::env_var_registry;

#[cfg(test)]
#[path = "config_schema_registry_tests.rs"]
mod tests;
