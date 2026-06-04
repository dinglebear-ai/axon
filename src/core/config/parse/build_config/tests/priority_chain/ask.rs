#![allow(clippy::needless_pass_by_value)]

use super::super::*;

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn ask_explain_cli_sets_explain_and_diagnostics() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut cfg = None;
    with_env_saved(&["AXON_LLM_COMPLETION_CONCURRENCY"], || unsafe {
        env::remove_var("AXON_LLM_COMPLETION_CONCURRENCY");
        cfg = Some(
            into_config_via_args(&["ask", "--explain", "claude marketplace plugins"]).unwrap(),
        );
    });
    let cfg = cfg.unwrap();

    assert!(cfg.ask_explain);
    assert!(cfg.ask_diagnostics);
    assert!(!cfg.ask_stream);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn ask_stream_cli_defaults_to_stream_without_diagnostics() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut cfg = None;
    with_env_saved(&["AXON_LLM_COMPLETION_CONCURRENCY"], || unsafe {
        env::remove_var("AXON_LLM_COMPLETION_CONCURRENCY");
        cfg = Some(into_config_via_args(&["ask", "what changed?"]).unwrap());
    });
    let cfg = cfg.unwrap();

    assert!(cfg.ask_stream);
    assert!(!cfg.ask_explain);
    assert!(!cfg.ask_diagnostics);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn ask_no_stream_cli_disables_default_stream() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut cfg = None;
    with_env_saved(&["AXON_LLM_COMPLETION_CONCURRENCY"], || unsafe {
        env::remove_var("AXON_LLM_COMPLETION_CONCURRENCY");
        cfg = Some(into_config_via_args(&["ask", "--no-stream", "what changed?"]).unwrap());
    });
    let cfg = cfg.unwrap();

    assert!(!cfg.ask_stream);
    assert!(!cfg.ask_explain);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn ask_follow_up_cli_sets_session_options() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut cfg = None;
    with_env_saved(&["AXON_LLM_COMPLETION_CONCURRENCY"], || unsafe {
        env::remove_var("AXON_LLM_COMPLETION_CONCURRENCY");
        cfg = Some(
            into_config_via_args(&[
                "ask",
                "--follow-up",
                "--session",
                "rust",
                "--reset-session",
                "what about tests?",
            ])
            .unwrap(),
        );
    });
    let cfg = cfg.unwrap();

    assert!(cfg.ask_follow_up);
    assert_eq!(cfg.ask_session.as_deref(), Some("rust"));
    assert!(cfg.ask_reset_session);
    assert!(cfg.ask_stream);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_chunk_limit_wins_over_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[ask]\nchunk-limit = 5").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_cl = env::var("AXON_ASK_CHUNK_LIMIT").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_ASK_CHUNK_LIMIT");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_cl {
            Some(v) => env::set_var("AXON_ASK_CHUNK_LIMIT", v),
            None => env::remove_var("AXON_ASK_CHUNK_LIMIT"),
        }
    }
    assert_eq!(
        cfg.unwrap().ask_chunk_limit,
        5,
        "TOML chunk-limit should override the default (20)"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_ask_chunk_limit() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[ask]\nchunk-limit = 5").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_cl = env::var("AXON_ASK_CHUNK_LIMIT").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("AXON_ASK_CHUNK_LIMIT", "8");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_cl {
            Some(v) => env::set_var("AXON_ASK_CHUNK_LIMIT", v),
            None => env::remove_var("AXON_ASK_CHUNK_LIMIT"),
        }
    }
    assert_eq!(
        cfg.unwrap().ask_chunk_limit,
        8,
        "env AXON_ASK_CHUNK_LIMIT=8 should override TOML chunk-limit=5"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_ask_authoritative_domains_and_boost_win_over_defaults() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[ask]\nauthoritative-domains = [\"code.claude.com\", \" docs.rs \"]\nauthoritative-boost = 0.12"
    )
    .unwrap();

    let mut got_domains = Vec::new();
    let mut got_boost = 0.0;
    with_env_saved(
        &[
            "AXON_CONFIG_PATH",
            "AXON_ASK_AUTHORITATIVE_DOMAINS",
            "AXON_ASK_AUTHORITATIVE_BOOST",
        ],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_ASK_AUTHORITATIVE_DOMAINS");
            env::remove_var("AXON_ASK_AUTHORITATIVE_BOOST");
            let cfg = into_config_via_args(&["status"]).unwrap();
            got_domains = cfg.ask_authoritative_domains;
            got_boost = cfg.ask_authoritative_boost;
        },
    );

    assert_eq!(got_domains, vec!["code.claude.com", "docs.rs"]);
    assert!((got_boost - 0.12).abs() < f64::EPSILON);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_ask_authoritative_domains_and_boost() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[ask]\nauthoritative-domains = [\"code.claude.com\"]\nauthoritative-boost = 0.12"
    )
    .unwrap();

    let mut got_domains = Vec::new();
    let mut got_boost = 0.0;
    with_env_saved(
        &[
            "AXON_CONFIG_PATH",
            "AXON_ASK_AUTHORITATIVE_DOMAINS",
            "AXON_ASK_AUTHORITATIVE_BOOST",
        ],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("AXON_ASK_AUTHORITATIVE_DOMAINS", "docs.rs,example.com");
            env::set_var("AXON_ASK_AUTHORITATIVE_BOOST", "0.2");
            let cfg = into_config_via_args(&["status"]).unwrap();
            got_domains = cfg.ask_authoritative_domains;
            got_boost = cfg.ask_authoritative_boost;
        },
    );

    assert_eq!(got_domains, vec!["docs.rs", "example.com"]);
    assert!((got_boost - 0.2).abs() < f64::EPSILON);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_ask_context_knobs_win_over_defaults() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[ask]\nmax-context-chars = 123456\nfull-docs = 7\nbackfill-chunks = 3\ndoc-fetch-concurrency = 9\ndoc-chunk-limit = 111\nmin-citations-nontrivial = 4"
    )
    .unwrap();

    let mut got = None;
    with_env_saved(
        &[
            "AXON_CONFIG_PATH",
            "AXON_ASK_MAX_CONTEXT_CHARS",
            "AXON_ASK_FULL_DOCS",
            "AXON_ASK_BACKFILL_CHUNKS",
            "AXON_ASK_DOC_FETCH_CONCURRENCY",
            "AXON_ASK_DOC_CHUNK_LIMIT",
            "AXON_ASK_MIN_CITATIONS_NONTRIVIAL",
        ],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_ASK_MAX_CONTEXT_CHARS");
            env::remove_var("AXON_ASK_FULL_DOCS");
            env::remove_var("AXON_ASK_BACKFILL_CHUNKS");
            env::remove_var("AXON_ASK_DOC_FETCH_CONCURRENCY");
            env::remove_var("AXON_ASK_DOC_CHUNK_LIMIT");
            env::remove_var("AXON_ASK_MIN_CITATIONS_NONTRIVIAL");
            got = Some(into_config_via_args(&["status"]).unwrap());
        },
    );
    let cfg = got.unwrap();

    assert_eq!(cfg.ask_max_context_chars, 123456);
    assert_eq!(cfg.ask_full_docs, 7);
    assert_eq!(cfg.ask_backfill_chunks, 3);
    assert_eq!(cfg.ask_doc_fetch_concurrency, 9);
    assert_eq!(cfg.ask_doc_chunk_limit, 111);
    assert_eq!(cfg.ask_min_citations_nontrivial, 4);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_ask_context_knobs() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[ask]\nmax-context-chars = 123456\nfull-docs = 7\nbackfill-chunks = 3\ndoc-fetch-concurrency = 9\ndoc-chunk-limit = 111\nmin-citations-nontrivial = 4"
    )
    .unwrap();

    let mut got = None;
    with_env_saved(
        &[
            "AXON_CONFIG_PATH",
            "AXON_ASK_MAX_CONTEXT_CHARS",
            "AXON_ASK_FULL_DOCS",
            "AXON_ASK_BACKFILL_CHUNKS",
            "AXON_ASK_DOC_FETCH_CONCURRENCY",
            "AXON_ASK_DOC_CHUNK_LIMIT",
            "AXON_ASK_MIN_CITATIONS_NONTRIVIAL",
        ],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("AXON_ASK_MAX_CONTEXT_CHARS", "222222");
            env::set_var("AXON_ASK_FULL_DOCS", "8");
            env::set_var("AXON_ASK_BACKFILL_CHUNKS", "4");
            env::set_var("AXON_ASK_DOC_FETCH_CONCURRENCY", "10");
            env::set_var("AXON_ASK_DOC_CHUNK_LIMIT", "222");
            env::set_var("AXON_ASK_MIN_CITATIONS_NONTRIVIAL", "5");
            got = Some(into_config_via_args(&["status"]).unwrap());
        },
    );
    let cfg = got.unwrap();

    assert_eq!(cfg.ask_max_context_chars, 222222);
    assert_eq!(cfg.ask_full_docs, 8);
    assert_eq!(cfg.ask_backfill_chunks, 4);
    assert_eq!(cfg.ask_doc_fetch_concurrency, 10);
    assert_eq!(cfg.ask_doc_chunk_limit, 222);
    assert_eq!(cfg.ask_min_citations_nontrivial, 5);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_ask_chunk_limit_clamps_lower_bound() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[ask]\nchunk-limit = 1").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_ASK_CHUNK_LIMIT"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_ASK_CHUNK_LIMIT");
        got = into_config_via_args(&["status"]).unwrap().ask_chunk_limit;
    });
    assert_eq!(got, 3);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_ask_chunk_limit_clamps_upper_bound() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[ask]\nchunk-limit = 999").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_ASK_CHUNK_LIMIT"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_ASK_CHUNK_LIMIT");
        got = into_config_via_args(&["status"]).unwrap().ask_chunk_limit;
    });
    assert_eq!(got, 64);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_hybrid_disabled_wins_over_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[search]\nhybrid-enabled = false").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_hs = env::var("AXON_HYBRID_SEARCH").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_HYBRID_SEARCH");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_hs {
            Some(v) => env::set_var("AXON_HYBRID_SEARCH", v),
            None => env::remove_var("AXON_HYBRID_SEARCH"),
        }
    }
    assert!(
        !cfg.unwrap().hybrid_search_enabled,
        "TOML hybrid-enabled=false should override the default (true)"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_hybrid_enabled() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[search]\nhybrid-enabled = false").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_hs = env::var("AXON_HYBRID_SEARCH").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("AXON_HYBRID_SEARCH", "true");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_hs {
            Some(v) => env::set_var("AXON_HYBRID_SEARCH", v),
            None => env::remove_var("AXON_HYBRID_SEARCH"),
        }
    }
    assert!(
        cfg.unwrap().hybrid_search_enabled,
        "env AXON_HYBRID_SEARCH=true should override TOML hybrid-enabled=false"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_ask_candidate_limit_wins_over_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[ask]\ncandidate-limit = 50").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_cl = env::var("AXON_ASK_CANDIDATE_LIMIT").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_ASK_CANDIDATE_LIMIT");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_cl {
            Some(v) => env::set_var("AXON_ASK_CANDIDATE_LIMIT", v),
            None => env::remove_var("AXON_ASK_CANDIDATE_LIMIT"),
        }
    }
    assert_eq!(
        cfg.unwrap().ask_candidate_limit,
        50,
        "TOML candidate-limit should override the default (250)"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_ask_candidate_limit_clamps_lower_bound() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[ask]\ncandidate-limit = 1").unwrap();
    let mut got = 0usize;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_ASK_CANDIDATE_LIMIT"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_ASK_CANDIDATE_LIMIT");
            got = into_config_via_args(&["status"])
                .unwrap()
                .ask_candidate_limit;
        },
    );
    assert_eq!(got, 8);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_ask_candidate_limit_clamps_upper_bound() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[ask]\ncandidate-limit = 999").unwrap();
    let mut got = 0usize;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_ASK_CANDIDATE_LIMIT"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_ASK_CANDIDATE_LIMIT");
            got = into_config_via_args(&["status"])
                .unwrap()
                .ask_candidate_limit;
        },
    );
    assert_eq!(got, 300);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_ask_min_relevance_score_wins_over_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[ask]\nmin-relevance-score = 0.7").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_mrs = env::var("AXON_ASK_MIN_RELEVANCE_SCORE").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_ASK_MIN_RELEVANCE_SCORE");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_mrs {
            Some(v) => env::set_var("AXON_ASK_MIN_RELEVANCE_SCORE", v),
            None => env::remove_var("AXON_ASK_MIN_RELEVANCE_SCORE"),
        }
    }
    let score = cfg.unwrap().ask_min_relevance_score;
    assert!(
        (score - 0.7).abs() < 1e-10,
        "TOML min-relevance-score=0.7 should override the default (0.45), got {score}"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_ask_min_relevance_score_clamps_lower_bound() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[ask]\nmin-relevance-score = -9.0").unwrap();
    let mut got = 0.0f64;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_ASK_MIN_RELEVANCE_SCORE"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_ASK_MIN_RELEVANCE_SCORE");
            got = into_config_via_args(&["status"])
                .unwrap()
                .ask_min_relevance_score;
        },
    );
    assert!((got - -1.0).abs() < 1e-10, "got {got}");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_ask_min_relevance_score_clamps_upper_bound() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[ask]\nmin-relevance-score = 9.0").unwrap();
    let mut got = 0.0f64;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_ASK_MIN_RELEVANCE_SCORE"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_ASK_MIN_RELEVANCE_SCORE");
            got = into_config_via_args(&["status"])
                .unwrap()
                .ask_min_relevance_score;
        },
    );
    assert!((got - 2.0).abs() < 1e-10, "got {got}");
}
