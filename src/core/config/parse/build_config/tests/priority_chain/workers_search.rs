#![allow(clippy::needless_pass_by_value)]

use super::super::*;

// --- [workers] + [search] (bead 2j9.4) priority-chain tests ---

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_ingest_lanes_wins_over_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[workers]\ningest-lanes = 7").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_INGEST_LANES"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_INGEST_LANES");
        got = into_config(cli_with_services(&["status"]))
            .unwrap()
            .ingest_lanes;
    });
    assert_eq!(got, 7, "TOML ingest-lanes=7 should override default (2)");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_workers_ingest_lanes() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[workers]\ningest-lanes = 7").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_INGEST_LANES"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("AXON_INGEST_LANES", "12");
        got = into_config(cli_with_services(&["status"]))
            .unwrap()
            .ingest_lanes;
    });
    assert_eq!(got, 12, "env AXON_INGEST_LANES=12 should override TOML=7");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_max_pending_crawl_clamps_out_of_range() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[workers]\nmax-pending-crawl-jobs = 99999999").unwrap();
    let mut got = 0usize;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_MAX_PENDING_CRAWL_JOBS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_MAX_PENDING_CRAWL_JOBS");
            got = into_config(cli_with_services(&["status"]))
                .unwrap()
                .max_pending_crawl_jobs;
        },
    );
    assert_eq!(got, 10_000, "TOML cap should clamp to 10_000 upper bound");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_max_pending_embed_wins_over_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[workers]\nmax-pending-embed-jobs = 25").unwrap();
    let mut got = 0usize;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_MAX_PENDING_EMBED_JOBS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_MAX_PENDING_EMBED_JOBS");
            got = into_config(cli_with_services(&["status"]))
                .unwrap()
                .max_pending_embed_jobs;
        },
    );
    assert_eq!(got, 25, "TOML embed cap=25 should override default (50)");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_max_pending_extract_wins_over_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[workers]\nmax-pending-extract-jobs = 11").unwrap();
    let mut got = 0usize;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_MAX_PENDING_EXTRACT_JOBS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_MAX_PENDING_EXTRACT_JOBS");
            got = into_config(cli_with_services(&["status"]))
                .unwrap()
                .max_pending_extract_jobs;
        },
    );
    assert_eq!(got, 11);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_max_pending_ingest_wins_over_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[workers]\nmax-pending-ingest-jobs = 13").unwrap();
    let mut got = 0usize;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_MAX_PENDING_INGEST_JOBS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_MAX_PENDING_INGEST_JOBS");
            got = into_config(cli_with_services(&["status"]))
                .unwrap()
                .max_pending_ingest_jobs;
        },
    );
    assert_eq!(got, 13);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_embed_doc_timeout_secs_wins_over_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[workers]\nembed-doc-timeout-secs = 600").unwrap();
    let mut got = 0u64;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_EMBED_DOC_TIMEOUT_SECS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_EMBED_DOC_TIMEOUT_SECS");
            got = into_config(cli_with_services(&["status"]))
                .unwrap()
                .embed_doc_timeout_secs;
        },
    );
    assert_eq!(got, 600);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_search_hnsw_ef_wins_over_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[search]\nhnsw-ef = 256").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_HNSW_EF_SEARCH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_HNSW_EF_SEARCH");
        got = into_config(cli_with_services(&["status"]))
            .unwrap()
            .hnsw_ef_search;
    });
    assert_eq!(got, 256, "TOML hnsw-ef=256 should override default (128)");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_search_hnsw_ef() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[search]\nhnsw-ef = 256").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_HNSW_EF_SEARCH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("AXON_HNSW_EF_SEARCH", "64");
        got = into_config(cli_with_services(&["status"]))
            .unwrap()
            .hnsw_ef_search;
    });
    assert_eq!(got, 64, "env wins over TOML");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_search_hnsw_ef_clamps_out_of_range() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[search]\nhnsw-ef = 9999").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_HNSW_EF_SEARCH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_HNSW_EF_SEARCH");
        got = into_config(cli_with_services(&["status"]))
            .unwrap()
            .hnsw_ef_search;
    });
    assert_eq!(
        got, 512,
        "TOML hnsw-ef=9999 should clamp to 512 upper bound"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_search_hnsw_ef_legacy_wins_over_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[search]\nhnsw-ef-legacy = 200").unwrap();
    let mut got = 0usize;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_HNSW_EF_SEARCH_LEGACY"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_HNSW_EF_SEARCH_LEGACY");
            got = into_config(cli_with_services(&["status"]))
                .unwrap()
                .hnsw_ef_search_legacy;
        },
    );
    assert_eq!(got, 200);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_search_collection_wins_over_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[search]\ncollection = \"toml_col\"").unwrap();
    let mut got = String::new();
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_COLLECTION"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_COLLECTION");
        got = into_config(cli_with_services(&["status"]))
            .unwrap()
            .collection;
    });
    assert_eq!(got, "toml_col");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_search_collection() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[search]\ncollection = \"toml_col\"").unwrap();
    let mut got = String::new();
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_COLLECTION"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("AXON_COLLECTION", "env_col");
        got = into_config(cli_with_services(&["status"]))
            .unwrap()
            .collection;
    });
    assert_eq!(got, "env_col");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn cli_wins_over_env_and_toml_for_collection() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[search]\ncollection = \"toml_col\"").unwrap();
    let mut got = String::new();
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_COLLECTION"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("AXON_COLLECTION", "env_col");
        got = into_config(cli_with_services(&["--collection", "cli_col", "status"]))
            .unwrap()
            .collection;
    });
    assert_eq!(got, "cli_col");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_search_collection_invalid_returns_err() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[search]\ncollection = \"evil; DROP\"").unwrap();
    let mut err_msg = String::new();
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_COLLECTION"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_COLLECTION");
        err_msg = into_config(cli_with_services(&["status"])).unwrap_err();
    });
    assert!(
        err_msg.contains("invalid collection name"),
        "expected invalid-collection error, got: {err_msg}"
    );
}
