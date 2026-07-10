use super::*;
use std::collections::BTreeSet;

#[test]
fn config_key_registry_has_all_contract_keys() {
    let keys: BTreeSet<&str> = config_key_registry().iter().map(|spec| spec.key).collect();
    assert!(
        keys.len() >= 24,
        "expected at least 24 required config keys, got {}",
        keys.len()
    );
    for required in [
        "server.default_collection",
        "pipeline.max_active_source_jobs",
        "jobs.heartbeat_secs",
        "sources.embed_by_default",
        "watch.tick_secs",
        "providers.embedding.batch_size",
        "providers.vector.write_concurrency",
        "providers.llm.completion_concurrency",
        "retrieval.hybrid_candidates",
        "crawl.max_pages",
        "memory.decay_enabled",
        "graph.enabled",
        "prune.retention_days.jobs",
        "observability.log_level",
        "security.allow_private_network_fetch",
    ] {
        assert!(
            keys.contains(required),
            "missing required config key {required}"
        );
    }
}

#[test]
fn config_key_registry_has_no_duplicate_keys() {
    let mut keys: Vec<&str> = config_key_registry().iter().map(|spec| spec.key).collect();
    let before = keys.len();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(before, keys.len(), "config key registry has duplicate keys");
}

#[test]
fn config_key_defaults_are_valid_json() {
    for spec in config_key_registry() {
        assert!(
            serde_json::from_str::<serde_json::Value>(spec.default_json).is_ok(),
            "config key {} has invalid default JSON: {}",
            spec.key,
            spec.default_json
        );
    }
}

#[test]
fn env_var_registry_has_all_contract_vars() {
    let names: BTreeSet<&str> = env_var_registry().iter().map(|spec| spec.name).collect();
    assert!(
        names.len() >= 20,
        "expected at least 20 required env vars, got {}",
        names.len()
    );
    for required in [
        "AXON_DATA_DIR",
        "QDRANT_URL",
        "TEI_URL",
        "AXON_CHROME_REMOTE_URL",
        "AXON_HTTP_HOST",
        "AXON_HTTP_PORT",
        "AXON_PUBLIC_URL",
        "AXON_HTTP_TOKEN",
        "AXON_AUTH_MODE",
        "AXON_GOOGLE_CLIENT_ID",
        "AXON_GOOGLE_CLIENT_SECRET",
        "GITHUB_TOKEN",
        "GITLAB_TOKEN",
        "GITEA_TOKEN",
        "REDDIT_CLIENT_ID",
        "REDDIT_CLIENT_SECRET",
        "TAVILY_API_KEY",
        "AXON_SEARXNG_URL",
        "AXON_OPENAI_API_KEY",
        "AXON_OPENAI_BASE_URL",
        "AXON_CODEX_HOME",
    ] {
        assert!(
            names.contains(required),
            "missing required env var {required}"
        );
    }
}

#[test]
fn env_var_registry_has_no_duplicate_names() {
    let mut names: Vec<&str> = env_var_registry().iter().map(|spec| spec.name).collect();
    let before = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(before, names.len(), "env var registry has duplicate names");
}

#[test]
fn env_var_registry_marks_secrets_not_example_allowed() {
    for spec in env_var_registry() {
        if spec.secret {
            assert!(
                !spec.example_allowed,
                "secret env var {} should not be example-allowed",
                spec.name
            );
        }
    }
}

#[test]
fn required_config_sections_match_contract_count() {
    assert_eq!(REQUIRED_CONFIG_SECTIONS.len(), 15);
    for section in [
        "server",
        "sources",
        "pipeline",
        "watch",
        "jobs",
        "ask",
        "artifacts",
    ] {
        assert!(
            REQUIRED_CONFIG_SECTIONS.contains(&section),
            "missing required top-level section {section}"
        );
    }
}

#[test]
fn config_key_sections_are_all_required_sections() {
    let required: BTreeSet<&str> = REQUIRED_CONFIG_SECTIONS.iter().copied().collect();
    for spec in config_key_registry() {
        assert!(
            required.contains(spec.section),
            "config key {} has section {} not in the required section list",
            spec.key,
            spec.section
        );
    }
}

#[test]
fn config_keys_and_env_vars_do_not_reuse_removed_names() {
    let removed = crate::schemas::removed::removed_surface_registry().config_keys;
    let removed_names: BTreeSet<&str> = removed.iter().map(|op| op.name).collect();
    for spec in config_key_registry() {
        assert!(
            !removed_names.contains(spec.key),
            "config key {} reuses a removed surface name",
            spec.key
        );
        if let Some(env) = spec.env_override {
            assert!(
                !removed_names.contains(env),
                "config key {} env_override {} reuses a removed surface name",
                spec.key,
                env
            );
        }
    }
    for spec in env_var_registry() {
        assert!(
            !removed_names.contains(spec.name),
            "env var {} reuses a removed surface name",
            spec.name
        );
    }
}
