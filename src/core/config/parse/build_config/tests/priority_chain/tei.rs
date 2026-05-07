#![allow(clippy::needless_pass_by_value)]

use super::super::*;

// --- [tei] priority-chain tests ---

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn toml_tei_max_retries_wins_over_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[tei]\nmax-retries = 3").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_mr = env::var("TEI_MAX_RETRIES").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("TEI_MAX_RETRIES");
    }
    let cfg = into_config(cli_with_services(&["status"]));
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
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[tei]\nmax-retries = 3").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_mr = env::var("TEI_MAX_RETRIES").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("TEI_MAX_RETRIES", "8");
    }
    let cfg = into_config(cli_with_services(&["status"]));
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
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[tei]\nmax-retries = 999").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_mr = env::var("TEI_MAX_RETRIES").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("TEI_MAX_RETRIES");
    }
    let cfg = into_config(cli_with_services(&["status"]));
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
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[tei]\nrequest-timeout-ms = 45000").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_to = env::var("TEI_REQUEST_TIMEOUT_MS").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("TEI_REQUEST_TIMEOUT_MS");
    }
    let cfg = into_config(cli_with_services(&["status"]));
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
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[tei]\nrequest-timeout-ms = 45000").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_to = env::var("TEI_REQUEST_TIMEOUT_MS").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("TEI_REQUEST_TIMEOUT_MS", "60000");
    }
    let cfg = into_config(cli_with_services(&["status"]));
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
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    // Below the 1000 lower bound — should clamp UP.
    writeln!(f, "[tei]\nrequest-timeout-ms = 50").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_to = env::var("TEI_REQUEST_TIMEOUT_MS").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("TEI_REQUEST_TIMEOUT_MS");
    }
    let cfg = into_config(cli_with_services(&["status"]));
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
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[tei]\nmax-client-batch-size = 96").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_bs = env::var("TEI_MAX_CLIENT_BATCH_SIZE").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("TEI_MAX_CLIENT_BATCH_SIZE");
    }
    let cfg = into_config(cli_with_services(&["status"]));
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
        "TOML tei.max-client-batch-size=96 should override the default (64)"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn env_wins_over_toml_for_tei_max_client_batch_size() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[tei]\nmax-client-batch-size = 96").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_bs = env::var("TEI_MAX_CLIENT_BATCH_SIZE").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::set_var("TEI_MAX_CLIENT_BATCH_SIZE", "32");
    }
    let cfg = into_config(cli_with_services(&["status"]));
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
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    writeln!(f, "[tei]\nmax-client-batch-size = 500").unwrap();

    let saved = env::var("AXON_CONFIG_PATH").ok();
    let saved_bs = env::var("TEI_MAX_CLIENT_BATCH_SIZE").ok();
    unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("TEI_MAX_CLIENT_BATCH_SIZE");
    }
    let cfg = into_config(cli_with_services(&["status"]));
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
        128,
        "out-of-range TOML max-client-batch-size=500 should clamp to 128 (upper bound)"
    );
}
