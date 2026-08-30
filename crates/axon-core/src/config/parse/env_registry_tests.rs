use super::*;
use EnvClassification::{Delete, KeepEnv, MoveToml};
use std::collections::BTreeSet;

#[test]
fn service_urls_are_env_not_toml() {
    for key in ["QDRANT_URL", "TEI_URL", "AXON_CHROME_REMOTE_URL"] {
        let spec = spec_for(key).expect("registered key");
        assert_eq!(spec.classification, KeepEnv);
        assert_eq!(spec.toml_destination, None);
    }
}

#[test]
fn removed_aliases_are_delete_on_migration() {
    for key in [
        "AXON_OPENAI_MODEL",
        "AXON_MCP_EMBED_ALLOWED_ROOTS",
        "AXON_HNSW_EF_SEARCH_LEGACY",
    ] {
        let spec = spec_for(key).expect("removed key remains registered for migration");
        assert_eq!(spec.classification, Delete, "{key}");
        assert_eq!(
            spec.legacy_behavior,
            LegacyBehavior::DeleteOnMigration,
            "{key}"
        );
        assert_eq!(spec.toml_destination, None, "{key}");
    }
    let canonical = spec_for("AXON_SOURCE_LOCAL_ALLOWED_ROOTS").expect("canonical local roots");
    assert_eq!(canonical.classification, KeepEnv);
    assert_eq!(canonical.legacy_behavior, LegacyBehavior::Canonical);
}

#[test]
fn moved_tuning_has_toml_destination() {
    for spec in all_specs() {
        if spec.classification == MoveToml {
            assert!(
                spec.toml_destination.is_some(),
                "{} is move-toml without destination",
                spec.key
            );
        }
    }
}

#[test]
fn embedding_tuning_env_keys_map_to_clean_break_provider_section() {
    // The clean-break parser accepts embedding tuning only under
    // `[providers.embedding]`. Legacy `[tei]`/`[embed]` sections are
    // rejected, so remediation hints must never point there.
    let expected = [
        (
            "TEI_MAX_CLIENT_BATCH_SIZE",
            "providers.embedding.batch-size",
        ),
        ("TEI_MAX_RETRIES", "providers.embedding.max-retries"),
        (
            "TEI_REQUEST_TIMEOUT_MS",
            "providers.embedding.request-timeout-ms",
        ),
        (
            "AXON_TEI_RETRY_BACKOFF_MS",
            "providers.embedding.retry-backoff-ms",
        ),
        (
            "AXON_TEI_COOLDOWN_AFTER_FAILURES",
            "providers.embedding.cooldown-after-failures",
        ),
        (
            "AXON_TEI_COOLDOWN_SECS",
            "providers.embedding.cooldown-secs",
        ),
        (
            "AXON_TEI_INTERACTIVE_RESERVED_REQUESTS",
            "providers.embedding.interactive-reserved-requests",
        ),
        (
            "AXON_TEI_BACKGROUND_MAX_CONCURRENT_REQUESTS",
            "providers.embedding.background-max-concurrent-requests",
        ),
        (
            "AXON_TEI_MAINTENANCE_MAX_CONCURRENT_REQUESTS",
            "providers.embedding.maintenance-max-concurrent-requests",
        ),
        (
            "AXON_TEI_QUERY_INSTRUCTION_ENABLED",
            "providers.embedding.query-instruction-enabled",
        ),
        (
            "AXON_TEI_MAX_CONCURRENT",
            "providers.embedding.max-concurrent-requests",
        ),
        (
            "AXON_TEI_MAX_IN_FLIGHT_INPUTS",
            "providers.embedding.max-in-flight-inputs",
        ),
        (
            "AXON_EMBED_CACHE_ENABLED",
            "providers.embedding.cache-enabled",
        ),
        (
            "AXON_EMBED_CACHE_MAX_ENTRIES",
            "providers.embedding.cache-max-entries",
        ),
        (
            "AXON_EMBED_POOL_MAX_INPUTS",
            "providers.embedding.pool-max-inputs",
        ),
        (
            "AXON_TEI_CLIENT_MAX_BATCH_TOKENS",
            "providers.embedding.max-batch-tokens",
        ),
        (
            "AXON_EMBED_SCHEDULER_ENABLED",
            "providers.embedding.scheduler-enabled",
        ),
        (
            "AXON_VECTOR_UPSERT_EMBED_OVERLAP",
            "providers.embedding.vector-upsert-overlap-enabled",
        ),
        (
            "AXON_EMBED_PREP_CONCURRENCY",
            "providers.embedding.prep-concurrency",
        ),
        (
            "AXON_EMBED_MAX_CHUNKS_PER_DOC",
            "providers.embedding.max-chunks-per-doc",
        ),
        (
            "AXON_EMBED_MAX_SOURCE_CHUNKS_PER_DOC",
            "providers.embedding.max-source-chunks-per-doc",
        ),
        (
            "AXON_EMBED_DEDUPE_EXACT_CHUNKS",
            "providers.embedding.dedupe-exact-chunks",
        ),
        (
            "AXON_OPENAI_EMBEDDING_MODEL",
            "providers.embedding.openai-model",
        ),
        (
            "AXON_OPENAI_EMBED_MAX_CLIENT_BATCH_SIZE",
            "providers.embedding.openai-max-client-batch-size",
        ),
        (
            "AXON_OPENAI_EMBED_MAX_CONCURRENT",
            "providers.embedding.openai-max-concurrent",
        ),
        (
            "AXON_OPENAI_EMBED_MAX_IN_FLIGHT_INPUTS",
            "providers.embedding.openai-max-in-flight-inputs",
        ),
        (
            "AXON_OPENAI_EMBED_POOL_MAX_INPUTS",
            "providers.embedding.openai-pool-max-inputs",
        ),
    ];

    for (key, destination) in expected {
        let spec = spec_for(key).expect("registered key");
        assert_eq!(spec.classification, MoveToml);
        assert_eq!(
            spec.toml_destination,
            Some(destination),
            "{key} should map to {destination}"
        );
    }
}

#[test]
fn implemented_env_keys_are_registered() {
    let required = [
        "AXON_SEARXNG_URL",
        "AXON_TEI_MAX_CONCURRENT",
        "AXON_TEI_MAX_IN_FLIGHT_INPUTS",
        "AXON_EMBED_POOL_MAX_INPUTS",
        "AXON_TEI_CLIENT_MAX_BATCH_TOKENS",
        "AXON_EMBED_SCHEDULER_ENABLED",
        "AXON_VECTOR_UPSERT_EMBED_OVERLAP",
        "AXON_EMBED_PREP_CONCURRENCY",
        "AXON_EMBED_MAX_CHUNKS_PER_DOC",
        "AXON_EMBED_MAX_SOURCE_CHUNKS_PER_DOC",
        "AXON_EMBED_DEDUPE_EXACT_CHUNKS",
        "AXON_OPENAI_EMBEDDING_MODEL",
        "AXON_OPENAI_EMBED_MAX_CLIENT_BATCH_SIZE",
        "AXON_OPENAI_EMBED_MAX_CONCURRENT",
        "AXON_OPENAI_EMBED_MAX_IN_FLIGHT_INPUTS",
        "AXON_OPENAI_EMBED_POOL_MAX_INPUTS",
        "AXON_MARKDOWN_CHUNK_MIN_CHARS",
        "AXON_MARKDOWN_CHUNK_MAX_CHARS",
        "AXON_CHUNK_OVERLAP_CHARS",
        "AXON_QDRANT_UPSERT_BATCH_SIZE",
        "AXON_QDRANT_UPSERT_PARALLELISM",
        "AXON_QDRANT_BULK_LOAD",
        "AXON_QDRANT_BULK_INDEXING_THRESHOLD_KB",
        "AXON_QDRANT_INDEXING_THRESHOLD_KB",
        "AXON_QDRANT_HNSW_M",
        "AXON_QDRANT_HNSW_EF_CONSTRUCT",
        "AXON_QDRANT_PAYLOAD_INDEX_PROFILE",
        "AXON_QDRANT_PAYLOAD_INDEX_PARALLELISM",
        "AXON_CODE_SEARCH_ALLOWED_ROOTS",
        "AXON_CODE_SEARCH_FRESHNESS_TTL_SECS",
        "AXON_CODE_SEARCH_REINDEX_TIMEOUT_SECS",
        "AXON_CODE_SEARCH_MAX_FILE_BYTES",
        "AXON_CODE_SEARCH_CHANGED_FILE_BATCH_SIZE",
        "AXON_WATCH_TICK_SECS",
        "AXON_WATCH_LEASE_SECS",
        "AXON_MCP_EMBED_MAX_LOCAL_BYTES",
        "AXON_MCP_EMBED_MAX_LOCAL_DEPTH",
        "AXON_MCP_EMBED_MAX_LOCAL_ENTRIES",
        "AXON_SOURCE_LOCAL_ALLOWED_ROOTS",
    ];

    let registered: BTreeSet<_> = all_specs().map(|spec| spec.key).collect();

    let missing: Vec<_> = required
        .iter()
        .copied()
        .filter(|key| !registered.contains(key))
        .collect();

    assert!(
        missing.is_empty(),
        "missing env_registry entries: {missing:?}"
    );
}

#[test]
fn env_example_does_not_include_toml_tuning_keys() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root");
    let env_example =
        std::fs::read_to_string(repo_root.join(".env.example")).expect("read root .env.example");
    let example_keys: BTreeSet<String> = env_example
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            line.split_once('=').map(|(key, _)| key.trim().to_string())
        })
        .collect();

    let moved_keys: BTreeSet<_> = all_specs()
        .filter(|spec| spec.classification == MoveToml)
        .map(|spec| spec.key)
        .collect();
    let drift: Vec<_> = example_keys
        .iter()
        .filter(|key| moved_keys.contains(key.as_str()))
        .cloned()
        .collect();

    assert!(
        drift.is_empty(),
        ".env.example contains TOML-owned keys: {drift:?}"
    );
}
