#![allow(clippy::needless_pass_by_value)]

use super::super::*;

// --- [tei] priority-chain tests ---

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_tei_max_retries_wins_over_default() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.embedding]\nmax-retries = 3").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_mr = env::var("TEI_MAX_RETRIES").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("TEI_MAX_RETRIES");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_mr {
            Some(v) => env::set_var("TEI_MAX_RETRIES", v),
            None => env::remove_var("TEI_MAX_RETRIES"),
        }
    }
    assert_eq!(
        cfg.unwrap().tei_max_retries,
        3,
        "TOML tei.max-retries=3 should override the default (5)"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_tei_max_retries() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.embedding]\nmax-retries = 3").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_mr = env::var("TEI_MAX_RETRIES").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("TEI_MAX_RETRIES", "8");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_mr {
            Some(v) => env::set_var("TEI_MAX_RETRIES", v),
            None => env::remove_var("TEI_MAX_RETRIES"),
        }
    }
    assert_eq!(
        cfg.unwrap().tei_max_retries,
        8,
        "env TEI_MAX_RETRIES=8 should override TOML max-retries=3"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_tei_max_retries_clamps_out_of_range() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.embedding]\nmax-retries = 999").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_mr = env::var("TEI_MAX_RETRIES").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("TEI_MAX_RETRIES");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_mr {
            Some(v) => env::set_var("TEI_MAX_RETRIES", v),
            None => env::remove_var("TEI_MAX_RETRIES"),
        }
    }
    assert_eq!(
        cfg.unwrap().tei_max_retries,
        20,
        "out-of-range TOML max-retries=999 should clamp to 20 (upper bound)"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_tei_request_timeout_ms_wins_over_default() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.embedding]\nrequest-timeout-ms = 45000").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_to = env::var("TEI_REQUEST_TIMEOUT_MS").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("TEI_REQUEST_TIMEOUT_MS");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_to {
            Some(v) => env::set_var("TEI_REQUEST_TIMEOUT_MS", v),
            None => env::remove_var("TEI_REQUEST_TIMEOUT_MS"),
        }
    }
    assert_eq!(
        cfg.unwrap().tei_request_timeout_ms,
        45000,
        "TOML tei.request-timeout-ms=45000 should override the default (30000)"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_tei_request_timeout_ms() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.embedding]\nrequest-timeout-ms = 45000").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_to = env::var("TEI_REQUEST_TIMEOUT_MS").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("TEI_REQUEST_TIMEOUT_MS", "60000");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_to {
            Some(v) => env::set_var("TEI_REQUEST_TIMEOUT_MS", v),
            None => env::remove_var("TEI_REQUEST_TIMEOUT_MS"),
        }
    }
    assert_eq!(
        cfg.unwrap().tei_request_timeout_ms,
        60000,
        "env TEI_REQUEST_TIMEOUT_MS=60000 should override TOML request-timeout-ms=45000"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_tei_request_timeout_ms_clamps_out_of_range() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    // Below the 1000 lower bound — should clamp UP.
    writeln!(f, "[providers.embedding]\nrequest-timeout-ms = 50").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_to = env::var("TEI_REQUEST_TIMEOUT_MS").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("TEI_REQUEST_TIMEOUT_MS");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_to {
            Some(v) => env::set_var("TEI_REQUEST_TIMEOUT_MS", v),
            None => env::remove_var("TEI_REQUEST_TIMEOUT_MS"),
        }
    }
    assert_eq!(
        cfg.unwrap().tei_request_timeout_ms,
        1000,
        "out-of-range TOML request-timeout-ms=50 should clamp to 1000 (lower bound)"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_tei_max_client_batch_size_wins_over_default() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.embedding]\nbatch-size = 96").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_bs = env::var("TEI_MAX_CLIENT_BATCH_SIZE").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("TEI_MAX_CLIENT_BATCH_SIZE");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_bs {
            Some(v) => env::set_var("TEI_MAX_CLIENT_BATCH_SIZE", v),
            None => env::remove_var("TEI_MAX_CLIENT_BATCH_SIZE"),
        }
    }
    assert_eq!(
        cfg.unwrap().tei_max_client_batch_size,
        96,
        "TOML tei.max-client-batch-size=96 should match or override the default"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_tei_request_timeout_ms_clamps_upper_bound() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.embedding]\nrequest-timeout-ms = 999999").unwrap();
    let mut got = 0u64;
    with_env_saved(&["AXON_CONFIG_PATH", "TEI_REQUEST_TIMEOUT_MS"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("TEI_REQUEST_TIMEOUT_MS");
        got = into_config_via_args(&["status"])
            .unwrap()
            .tei_request_timeout_ms;
    });
    assert_eq!(got, 300_000);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_tei_max_client_batch_size_clamps_lower_bound() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.embedding]\nbatch-size = 0").unwrap();
    let mut got = 0usize;
    with_env_saved(
        &["AXON_CONFIG_PATH", "TEI_MAX_CLIENT_BATCH_SIZE"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("TEI_MAX_CLIENT_BATCH_SIZE");
            got = into_config_via_args(&["status"])
                .unwrap()
                .tei_max_client_batch_size;
        },
    );
    assert_eq!(got, 1);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_out_of_range_tei_max_retries_shadows_toml_and_clamps() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.embedding]\nmax-retries = 3").unwrap();
    let mut got = 0usize;
    with_env_saved(&["AXON_CONFIG_PATH", "TEI_MAX_RETRIES"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("TEI_MAX_RETRIES", "999");
        got = into_config_via_args(&["status"]).unwrap().tei_max_retries;
    });
    assert_eq!(got, 20, "parsed env values win even when clamped");
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_tei_max_client_batch_size() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.embedding]\nbatch-size = 96").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_bs = env::var("TEI_MAX_CLIENT_BATCH_SIZE").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("TEI_MAX_CLIENT_BATCH_SIZE", "32");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_bs {
            Some(v) => env::set_var("TEI_MAX_CLIENT_BATCH_SIZE", v),
            None => env::remove_var("TEI_MAX_CLIENT_BATCH_SIZE"),
        }
    }
    assert_eq!(
        cfg.unwrap().tei_max_client_batch_size,
        32,
        "env TEI_MAX_CLIENT_BATCH_SIZE=32 should override TOML max-client-batch-size=96"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_tei_max_client_batch_size_clamps_out_of_range() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[providers.embedding]\nbatch-size = 500").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_bs = env::var("TEI_MAX_CLIENT_BATCH_SIZE").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("TEI_MAX_CLIENT_BATCH_SIZE");
    }
    let cfg = into_config_via_args(&["status"]);
    unsafe {
        match saved {
            Some(v) => env::set_var("AXON_CONFIG_PATH", v),
            None => env::remove_var("AXON_CONFIG_PATH"),
        }
        match saved_bs {
            Some(v) => env::set_var("TEI_MAX_CLIENT_BATCH_SIZE", v),
            None => env::remove_var("TEI_MAX_CLIENT_BATCH_SIZE"),
        }
    }
    assert_eq!(
        cfg.unwrap().tei_max_client_batch_size,
        256,
        "out-of-range TOML max-client-batch-size=500 should clamp to 256 (upper bound)"
    );
}

// --- [providers.embedding] previously-silently-ignored keys (axon_rust-ldozg) ---
//
// `retry-backoff-ms`, `cooldown-after-failures`, `cooldown-secs`,
// `interactive-reserved-requests`, `background-max-concurrent-requests`,
// `maintenance-max-concurrent-requests`, and `query-instruction-enabled` were
// parsed by `RawEmbeddingSection` (round-tripped cleanly, no "unknown field"
// error) but `apply_providers()` never copied them onto the legacy
// `TomlConfig` shape, so no `Config` field ever read them — setting any of
// these in `config.toml` silently did nothing. This test sets every one of
// them to a non-default value and asserts the resulting runtime `Config`
// field equals that value (not the default), proving the full
// TOML -> RawTomlConfig -> legacy TomlConfig -> Config pipeline actually
// reads them now.
#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_embedding_previously_dead_keys_are_read_into_config() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[providers.embedding]\n\
         retry-backoff-ms = 750\n\
         cooldown-after-failures = 7\n\
         cooldown-secs = 45\n\
         interactive-reserved-requests = 2\n\
         background-max-concurrent-requests = 5\n\
         maintenance-max-concurrent-requests = 2\n\
         query-instruction-enabled = false"
    )
    .unwrap();

    let mut got: Option<Config> = None;
    with_env_saved(
        &[
            "AXON_CONFIG_PATH",
            "AXON_TEI_RETRY_BACKOFF_MS",
            "AXON_TEI_COOLDOWN_AFTER_FAILURES",
            "AXON_TEI_COOLDOWN_SECS",
            "AXON_TEI_INTERACTIVE_RESERVED_REQUESTS",
            "AXON_TEI_BACKGROUND_MAX_CONCURRENT_REQUESTS",
            "AXON_TEI_MAINTENANCE_MAX_CONCURRENT_REQUESTS",
            "AXON_TEI_QUERY_INSTRUCTION_ENABLED",
        ],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_TEI_RETRY_BACKOFF_MS");
            env::remove_var("AXON_TEI_COOLDOWN_AFTER_FAILURES");
            env::remove_var("AXON_TEI_COOLDOWN_SECS");
            env::remove_var("AXON_TEI_INTERACTIVE_RESERVED_REQUESTS");
            env::remove_var("AXON_TEI_BACKGROUND_MAX_CONCURRENT_REQUESTS");
            env::remove_var("AXON_TEI_MAINTENANCE_MAX_CONCURRENT_REQUESTS");
            env::remove_var("AXON_TEI_QUERY_INSTRUCTION_ENABLED");
            got = into_config_via_args(&["status"]).ok();
        },
    );
    let cfg = got.expect("config should build from valid TOML fixture");

    // Defaults (for contrast — none of these should equal the fixture value):
    // retry_backoff_ms=500, cooldown_after_failures=3, cooldown_secs=30,
    // interactive_reserved_requests=1, background_max_concurrent_requests=3,
    // maintenance_max_concurrent_requests=1, query_instruction_enabled=true.
    assert_eq!(cfg.embed_tei_retry_backoff_ms, 750);
    assert_eq!(cfg.embed_tei_cooldown_after_failures, 7);
    assert_eq!(cfg.embed_tei_cooldown_secs, 45);
    assert_eq!(cfg.embed_tei_interactive_reserved_requests, 2);
    assert_eq!(cfg.embed_tei_background_max_concurrent_requests, 5);
    assert_eq!(cfg.embed_tei_maintenance_max_concurrent_requests, 2);
    assert!(!cfg.embed_tei_query_instruction_enabled);
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_embedding_interactive_reserved_requests() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(
        f,
        "[providers.embedding]\ninteractive-reserved-requests = 2"
    )
    .unwrap();

    let mut got = 0usize;
    with_env_saved(
        &["AXON_CONFIG_PATH", "AXON_TEI_INTERACTIVE_RESERVED_REQUESTS"],
        || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("AXON_TEI_INTERACTIVE_RESERVED_REQUESTS", "9");
            got = into_config_via_args(&["status"])
                .unwrap()
                .embed_tei_interactive_reserved_requests;
        },
    );
    assert_eq!(
        got, 9,
        "env AXON_TEI_INTERACTIVE_RESERVED_REQUESTS=9 should override TOML interactive-reserved-requests=2"
    );
}
