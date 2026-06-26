use super::*;
use EnvClassification::{KeepEnv, MoveToml};

#[test]
fn service_urls_are_env_not_toml() {
    for key in ["QDRANT_URL", "TEI_URL", "AXON_CHROME_REMOTE_URL"] {
        let spec = spec_for(key).expect("registered key");
        assert_eq!(spec.classification, KeepEnv);
        assert_eq!(spec.toml_destination, None);
    }
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
fn implemented_env_keys_are_registered() {
    let required = [
        "AXON_SEARXNG_URL",
        "AXON_TEI_MAX_CONCURRENT",
        "AXON_TEI_MAX_IN_FLIGHT_INPUTS",
        "AXON_EMBED_POOL_MAX_INPUTS",
        "AXON_EMBED_PREP_CONCURRENCY",
        "AXON_EMBED_MAX_CHUNKS_PER_DOC",
        "AXON_EMBED_MAX_SOURCE_CHUNKS_PER_DOC",
        "AXON_EMBED_DEDUPE_EXACT_CHUNKS",
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
    ];

    let registered: std::collections::BTreeSet<_> = all_specs().map(|spec| spec.key).collect();

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
