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
