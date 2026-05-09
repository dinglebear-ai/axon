#![allow(clippy::needless_pass_by_value)]

use super::super::*;

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
    let cfg = into_config(cli_with_services(&["status"]));
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
        "TOML chunk-limit should override the default (10)"
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
    let cfg = into_config(cli_with_services(&["status"]));
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
fn toml_ask_chunk_limit_clamps_lower_bound() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[ask]\nchunk-limit = 1").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_ASK_CHUNK_LIMIT"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_ASK_CHUNK_LIMIT");
        got = into_config(cli_with_services(&["status"]))
            .unwrap()
            .ask_chunk_limit;
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
        got = into_config(cli_with_services(&["status"]))
            .unwrap()
            .ask_chunk_limit;
    });
    assert_eq!(got, 40);
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
    let cfg = into_config(cli_with_services(&["status"]));
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
    let cfg = into_config(cli_with_services(&["status"]));
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
    let cfg = into_config(cli_with_services(&["status"]));
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
        "TOML candidate-limit should override the default (150)"
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
            got = into_config(cli_with_services(&["status"]))
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
            got = into_config(cli_with_services(&["status"]))
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
    let cfg = into_config(cli_with_services(&["status"]));
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
            got = into_config(cli_with_services(&["status"]))
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
            got = into_config(cli_with_services(&["status"]))
                .unwrap()
                .ask_min_relevance_score;
        },
    );
    assert!((got - 2.0).abs() < 1e-10, "got {got}");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn server_url_trims_and_ignores_empty_values() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut got = false;
    with_env_saved(&["AXON_SERVER_URL", "AXON_ASK_SERVER_URL"], || unsafe {
        env::set_var("AXON_SERVER_URL", "   ");
        env::remove_var("AXON_ASK_SERVER_URL");
        got = into_config(cli_with_services(&["status"]))
            .unwrap()
            .server_url
            .is_none();
    });
    assert!(got);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn server_url_rejects_malformed_values() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut err = String::new();
    with_env_saved(&["AXON_SERVER_URL", "AXON_ASK_SERVER_URL"], || unsafe {
        env::set_var("AXON_SERVER_URL", "://not-a-url");
        env::remove_var("AXON_ASK_SERVER_URL");
        err = into_config(cli_with_services(&["status"])).unwrap_err();
    });
    assert!(err.contains("invalid --server-url / AXON_SERVER_URL"));
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn server_url_accepts_trimmed_values() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut got = String::new();
    with_env_saved(&["AXON_SERVER_URL", "AXON_ASK_SERVER_URL"], || unsafe {
        env::set_var("AXON_SERVER_URL", "  http://127.0.0.1:8001/base  ");
        env::remove_var("AXON_ASK_SERVER_URL");
        got = into_config(cli_with_services(&["status"]))
            .unwrap()
            .server_url
            .unwrap()
            .to_string();
    });
    assert_eq!(got, "http://127.0.0.1:8001/base");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn generic_server_url_env_enables_server_mode() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut got = None;
    let mut mode = crate::core::config::types::ClientMode::Local;
    with_env_saved(&["AXON_SERVER_URL", "AXON_ASK_SERVER_URL"], || unsafe {
        env::set_var("AXON_SERVER_URL", "http://127.0.0.1:8001");
        env::remove_var("AXON_ASK_SERVER_URL");
        let cfg = into_config(cli_with_services(&["status"])).unwrap();
        got = cfg.server_url.map(|url| url.to_string());
        mode = cfg.client_mode;
    });
    assert_eq!(got.as_deref(), Some("http://127.0.0.1:8001/"));
    assert_eq!(mode, crate::core::config::types::ClientMode::Server);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn cli_server_url_overrides_generic_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut got = String::new();
    with_env_saved(&["AXON_SERVER_URL", "AXON_ASK_SERVER_URL"], || unsafe {
        env::set_var("AXON_SERVER_URL", "http://127.0.0.1:8001/env");
        env::remove_var("AXON_ASK_SERVER_URL");
        got = into_config(cli_with_services(&[
            "--server-url",
            "http://127.0.0.1:9000/cli",
            "status",
        ]))
        .unwrap()
        .server_url
        .unwrap()
        .to_string();
    });
    assert_eq!(got, "http://127.0.0.1:9000/cli");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn ask_server_url_alias_used_only_when_generic_unset() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut alias = String::new();
    let mut generic = String::new();
    let mut status_url_is_none = false;
    with_env_saved(&["AXON_SERVER_URL", "AXON_ASK_SERVER_URL"], || unsafe {
        env::remove_var("AXON_SERVER_URL");
        env::set_var("AXON_ASK_SERVER_URL", "http://127.0.0.1:8001/ask-alias");
        alias = into_config(cli_with_services(&["ask", "what is indexed?"]))
            .unwrap()
            .server_url
            .unwrap()
            .to_string();
        status_url_is_none = into_config(cli_with_services(&["status"]))
            .unwrap()
            .server_url
            .is_none();

        env::set_var("AXON_SERVER_URL", "http://127.0.0.1:8001/generic");
        generic = into_config(cli_with_services(&["ask", "what is indexed?"]))
            .unwrap()
            .server_url
            .unwrap()
            .to_string();
    });
    assert_eq!(alias, "http://127.0.0.1:8001/ask-alias");
    assert!(status_url_is_none);
    assert_eq!(generic, "http://127.0.0.1:8001/generic");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn explicit_local_mode_bypasses_server_url() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut server_url_is_none = false;
    let mut local_mode = false;
    let mut mode = crate::core::config::types::ClientMode::Server;
    with_env_saved(
        &["AXON_SERVER_URL", "AXON_ASK_SERVER_URL", "AXON_LOCAL_MODE"],
        || unsafe {
            env::set_var("AXON_SERVER_URL", "http://127.0.0.1:8001");
            env::remove_var("AXON_ASK_SERVER_URL");
            env::remove_var("AXON_LOCAL_MODE");
            let cfg = into_config(cli_with_services(&["--local", "status"])).unwrap();
            server_url_is_none = cfg.server_url.is_none();
            local_mode = cfg.local_mode;
            mode = cfg.client_mode;
        },
    );
    assert!(server_url_is_none);
    assert!(local_mode);
    assert_eq!(mode, crate::core::config::types::ClientMode::Local);
}
