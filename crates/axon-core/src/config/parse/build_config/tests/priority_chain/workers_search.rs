#![allow(clippy::needless_pass_by_value)]

use super::super::*;

// --- [workers] + [search] (bead 2j9.4) priority-chain tests ---

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_ingest_lanes_wins_over_default() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\ningest-lanes = 7").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_INGEST_LANES"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_INGEST_LANES");
        got = into_config_via_args(&["status"]).unwrap().ingest_lanes;
    });
    assert_eq!(got, 7, "TOML ingest-lanes=7 should override default (2)");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_ingest_lanes_clamps_lower_bound() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\ningest-lanes = 0").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_INGEST_LANES"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_INGEST_LANES");
        got = into_config_via_args(&["status"]).unwrap().ingest_lanes;
    });
    assert_eq!(got, 1);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_ingest_lanes_clamps_upper_bound() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\ningest-lanes = 999").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_INGEST_LANES"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_INGEST_LANES");
        got = into_config_via_args(&["status"]).unwrap().ingest_lanes;
    });
    assert_eq!(got, 16);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_workers_ingest_lanes() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\ningest-lanes = 7").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_INGEST_LANES"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("AXON_INGEST_LANES", "12");
        got = into_config_via_args(&["status"]).unwrap().ingest_lanes;
    });
    assert_eq!(got, 12, "env AXON_INGEST_LANES=12 should override TOML=7");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_adaptive_concurrency_parses_min_and_max() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[crawl.adaptive-concurrency]\nenabled = true\nmin = 2\nmax = 32"
    )
    .unwrap();
    let mut got = None;
    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        got = Some(
            into_config_via_args(&["status"])
                .unwrap()
                .adaptive_concurrency,
        );
    });
    let got = got.expect("config captured");
    assert!(got.enabled);
    assert_eq!(got.min, 2);
    assert_eq!(got.max, Some(32));
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_adaptive_concurrency_normalizes_min_and_default_max() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[crawl]\ncrawl-concurrency-limit = 12\n\n[crawl.adaptive-concurrency]\nenabled = true\nmin = 0"
    )
    .unwrap();
    let mut got = None;
    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        got = Some(
            into_config_via_args(&["status"])
                .unwrap()
                .adaptive_concurrency,
        );
    });
    let got = got.expect("config captured");
    assert!(got.enabled);
    assert_eq!(got.min, 1);
    assert_eq!(got.max, Some(12));
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_chrome_remote_local_policy_parses() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.render]\nremote-local-policy = true").unwrap();
    let mut got = false;
    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        got = into_config_via_args(&["status"])
            .unwrap()
            .chrome_remote_local_policy;
    });
    assert!(got);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_adaptive_concurrency_rejects_min_greater_than_max() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[crawl.adaptive-concurrency]\nenabled = true\nmin = 33\nmax = 32"
    )
    .unwrap();
    let mut err_msg = String::new();
    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        err_msg = into_config_via_args(&["status"]).unwrap_err();
    });
    assert!(
        err_msg.contains("workers.adaptive-concurrency.min must be <= max"),
        "unexpected error: {err_msg}"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_adaptive_concurrency_rejects_max_above_broadcast_cap() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[crawl.adaptive-concurrency]\nenabled = true\nmin = 1\nmax = 1025"
    )
    .unwrap();
    let mut err_msg = String::new();
    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        err_msg = into_config_via_args(&["status"]).unwrap_err();
    });
    assert!(
        err_msg.contains(
            "workers.adaptive-concurrency.max must be <= min(crawl-broadcast-buffer-max, 1024)"
        ),
        "unexpected error: {err_msg}"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_adaptive_concurrency_rejects_unsupported_knobs() {
    let _guard = env_guard();
    let cases = [
        "decrease-factor = 0.25",
        "initial = 8",
        "sync-interval-ms = 250",
    ];
    for extra in cases {
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[crawl.adaptive-concurrency]\nenabled = true\n{extra}").unwrap();
        let mut err_msg = String::new();
        with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            err_msg = into_config_via_args(&["status"]).unwrap_err();
        });
        assert!(
            err_msg.contains("unknown field"),
            "expected unknown-field parse error for {extra}, got: {err_msg}"
        );
    }
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_embed_lanes_wins_over_default() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\nembed-lanes = 6").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_EMBED_LANES"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_EMBED_LANES");
        got = into_config_via_args(&["status"]).unwrap().embed_lanes;
    });
    assert_eq!(got, 6);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_workers_embed_lanes() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\nembed-lanes = 6").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_EMBED_LANES"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("AXON_EMBED_LANES", "9");
        got = into_config_via_args(&["status"]).unwrap().embed_lanes;
    });
    assert_eq!(got, 9);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_embed_lanes_clamps_bounds() {
    let _guard = env_guard();
    let mut low = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    let mut high = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(low, "[pipeline]\nembed-lanes = 0").unwrap();
    writeln!(high, "[pipeline]\nembed-lanes = 999").unwrap();
    let mut got_low = 0usize;
    let mut got_high = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_EMBED_LANES"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", low.path());
        env::remove_var("AXON_EMBED_LANES");
        got_low = into_config_via_args(&["status"]).unwrap().embed_lanes;
        env::set_var("AXON_CONFIG_PATH", high.path());
        got_high = into_config_via_args(&["status"]).unwrap().embed_lanes;
    });
    assert_eq!(got_low, 1);
    assert_eq!(got_high, 32);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_queue_summary_secs_allows_disable_and_env_override() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\nqueue-summary-secs = 0").unwrap();
    let mut got = 999u64;
    let mut env_got = 0u64;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_QUEUE_SUMMARY_SECS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_QUEUE_SUMMARY_SECS");
            got = into_config_via_args(&["status"])
                .unwrap()
                .queue_summary_secs;
            env::set_var("AXON_QUEUE_SUMMARY_SECS", "12");
            env_got = into_config_via_args(&["status"])
                .unwrap()
                .queue_summary_secs;
        },
    );
    assert_eq!(got, 0);
    assert_eq!(env_got, 12);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_qdrant_point_buffer_wins_and_clamps() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    let mut high = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\nqdrant-point-buffer = 1024").unwrap();
    writeln!(high, "[pipeline]\nqdrant-point-buffer = 999999").unwrap();
    let mut got = 0usize;
    let mut env_got = 0usize;
    let mut high_got = 0usize;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_QDRANT_POINT_BUFFER"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_QDRANT_POINT_BUFFER");
            got = into_config_via_args(&["status"])
                .unwrap()
                .qdrant_point_buffer;
            env::set_var("AXON_QDRANT_POINT_BUFFER", "2048");
            env_got = into_config_via_args(&["status"])
                .unwrap()
                .qdrant_point_buffer;
            env::remove_var("AXON_QDRANT_POINT_BUFFER");
            env::set_var("AXON_CONFIG_PATH", high.path());
            high_got = into_config_via_args(&["status"])
                .unwrap()
                .qdrant_point_buffer;
        },
    );
    assert_eq!(got, 1024);
    assert_eq!(env_got, 2048);
    assert_eq!(high_got, 16_384);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_max_pending_crawl_clamps_out_of_range() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\nmax-pending-crawl-jobs = 99999999").unwrap();
    let mut got = 0usize;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_MAX_PENDING_CRAWL_JOBS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_MAX_PENDING_CRAWL_JOBS");
            got = into_config_via_args(&["status"])
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
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\nmax-pending-embed-jobs = 25").unwrap();
    let mut got = 0usize;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_MAX_PENDING_EMBED_JOBS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_MAX_PENDING_EMBED_JOBS");
            got = into_config_via_args(&["status"])
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
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\nmax-pending-extract-jobs = 11").unwrap();
    let mut got = 0usize;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_MAX_PENDING_EXTRACT_JOBS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_MAX_PENDING_EXTRACT_JOBS");
            got = into_config_via_args(&["status"])
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
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\nmax-pending-ingest-jobs = 13").unwrap();
    let mut got = 0usize;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_MAX_PENDING_INGEST_JOBS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_MAX_PENDING_INGEST_JOBS");
            got = into_config_via_args(&["status"])
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
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\nembed-doc-timeout-secs = 600").unwrap();
    let mut got = 0u64;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_EMBED_DOC_TIMEOUT_SECS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_EMBED_DOC_TIMEOUT_SECS");
            got = into_config_via_args(&["status"])
                .unwrap()
                .embed_doc_timeout_secs;
        },
    );
    assert_eq!(got, 600);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_embed_doc_timeout_secs_clamps_lower_bound() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\nembed-doc-timeout-secs = 1").unwrap();
    let mut got = 0u64;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_EMBED_DOC_TIMEOUT_SECS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_EMBED_DOC_TIMEOUT_SECS");
            got = into_config_via_args(&["status"])
                .unwrap()
                .embed_doc_timeout_secs;
        },
    );
    assert_eq!(got, 30);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_workers_embed_doc_timeout_secs_clamps_upper_bound() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[pipeline]\nembed-doc-timeout-secs = 99999").unwrap();
    let mut got = 0u64;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_EMBED_DOC_TIMEOUT_SECS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_EMBED_DOC_TIMEOUT_SECS");
            got = into_config_via_args(&["status"])
                .unwrap()
                .embed_doc_timeout_secs;
        },
    );
    assert_eq!(got, 3600);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_search_hnsw_ef_wins_over_default() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.vector]\nhnsw-ef = 256").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_HNSW_EF_SEARCH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_HNSW_EF_SEARCH");
        got = into_config_via_args(&["status"]).unwrap().hnsw_ef_search;
    });
    assert_eq!(got, 256, "TOML hnsw-ef=256 should override default (128)");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_search_hnsw_ef() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.vector]\nhnsw-ef = 256").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_HNSW_EF_SEARCH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("AXON_HNSW_EF_SEARCH", "64");
        got = into_config_via_args(&["status"]).unwrap().hnsw_ef_search;
    });
    assert_eq!(got, 64, "env wins over TOML");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_search_hnsw_ef_clamps_out_of_range() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.vector]\nhnsw-ef = 9999").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_HNSW_EF_SEARCH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_HNSW_EF_SEARCH");
        got = into_config_via_args(&["status"]).unwrap().hnsw_ef_search;
    });
    assert_eq!(
        got, 512,
        "TOML hnsw-ef=9999 should clamp to 512 upper bound"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_search_hnsw_ef_clamps_lower_bound() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.vector]\nhnsw-ef = 1").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_HNSW_EF_SEARCH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_HNSW_EF_SEARCH");
        got = into_config_via_args(&["status"]).unwrap().hnsw_ef_search;
    });
    assert_eq!(got, 32);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_hnsw_ef_legacy_is_rejected() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.vector]\nhnsw-ef-legacy = 200").unwrap();
    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        let err = into_config_via_args(&["status"]).expect_err("removed key must fail");
        assert!(err.contains("hnsw-ef-legacy"), "unexpected error: {err}");
        assert!(err.contains("hnsw-ef"), "missing canonical key: {err}");
    });
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_search_collection_wins_over_default() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[server]\ndefault-collection = \"toml_col\"").unwrap();
    let mut got = String::new();
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_COLLECTION"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_COLLECTION");
        got = into_config_via_args(&["status"]).unwrap().collection;
    });
    assert_eq!(got, "toml_col");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_search_collection() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[server]\ndefault-collection = \"toml_col\"").unwrap();
    let mut got = String::new();
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_COLLECTION"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("AXON_COLLECTION", "env_col");
        got = into_config_via_args(&["status"]).unwrap().collection;
    });
    assert_eq!(got, "env_col");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn cli_wins_over_env_and_toml_for_collection() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[server]\ndefault-collection = \"toml_col\"").unwrap();
    let mut got = String::new();
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_COLLECTION"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("AXON_COLLECTION", "env_col");
        got = into_config_via_args(&["--collection", "cli_col", "status"])
            .unwrap()
            .collection;
    });
    assert_eq!(got, "cli_col");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_search_collection_invalid_returns_err() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[server]\ndefault-collection = \"evil; DROP\"").unwrap();
    let mut err_msg = String::new();
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_COLLECTION"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_COLLECTION");
        err_msg = into_config_via_args(&["status"]).unwrap_err();
    });
    assert!(
        err_msg.contains("invalid collection name"),
        "expected invalid-collection error, got: {err_msg}"
    );
}
