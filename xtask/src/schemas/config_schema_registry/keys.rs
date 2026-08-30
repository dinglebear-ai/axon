use super::{ConfigKeySpec, PROJECTION_BATCH_KEYS, RAW_CONFIG_KEYS};

pub fn config_key_registry() -> Vec<ConfigKeySpec> {
    let mut keys: Vec<_> = RAW_CONFIG_KEYS
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
        .collect();
    keys.extend(
        PROJECTION_BATCH_KEYS
            .iter()
            .map(|&(suffix, default_json, env, description)| ConfigKeySpec {
                key: Box::leak(format!("server.projection_batch.{suffix}").into_boxed_str()),
                section: "server",
                kind: "integer",
                default_json,
                owner_crate: "axon-core",
                env_override: Some(env),
                secret: false,
                restart_required: true,
                description,
            }),
    );
    keys
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
