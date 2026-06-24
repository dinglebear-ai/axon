use super::ask_doc_cache;
use axon_core::config::Config;

#[test]
fn ask_doc_cache_uses_runtime_cache_config() {
    let cfg = Config {
        ask_cache_max_capacity_bytes: 12_345,
        ask_cache_ttl_secs: 7,
        ..Config::default()
    };

    let cache = ask_doc_cache(&cfg);

    assert_eq!(cache.config().max_capacity_bytes, 12_345);
    assert_eq!(cache.config().effective_ttl_secs(), 7);
}
