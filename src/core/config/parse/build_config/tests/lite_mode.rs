//! AXON_LITE / MCP origin / URL-required / ACP env tests.
//! Test BODIES unchanged from the previous flat `mod tests` (bead 2j9.6).

#![allow(clippy::needless_pass_by_value)]

use super::*;

#[allow(unsafe_code)]
#[test]
fn into_config_reads_axon_lite_env_var() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        env::set_var("AXON_LITE", "1");
        env::set_var("QDRANT_URL", "http://localhost:53333");
        env::set_var("TEI_URL", "http://localhost:52000");
    }

    let cli = Cli::parse_from(["axon", "scrape", "https://example.com"]);
    let cfg = into_config(cli).expect("lite mode should not require PG/Redis/AMQP");
    assert!(cfg.lite_mode);

    unsafe {
        env::remove_var("AXON_LITE");
        env::remove_var("QDRANT_URL");
        env::remove_var("TEI_URL");
    }
}

#[allow(unsafe_code)]
#[test]
fn into_config_parses_mcp_origin_allowlist_from_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    const MCP: &str = "AXON_MCP_ALLOWED_ORIGINS";

    unsafe {
        env::set_var(MCP, " https://axon.example.com , http://localhost:49010 ");
    }

    let cli = Cli::parse_from([
        "axon",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "--tei-url",
        "http://127.0.0.1:52000",
        "status",
    ]);
    let cfg = into_config(cli).expect("status config should parse");

    assert_eq!(
        cfg.mcp_allowed_origins,
        vec![
            "https://axon.example.com".to_string(),
            "http://localhost:49010".to_string(),
        ]
    );

    unsafe {
        env::remove_var(MCP);
    }
}

#[test]
fn into_config_normalizes_tei_url_like_other_services() {
    let _guard = ENV_LOCK.lock().unwrap();
    let cli = Cli::parse_from([
        "axon",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "--tei-url",
        "http://axon-tei:80",
        "status",
    ]);
    let cfg = into_config(cli).expect("status config should parse");
    assert_eq!(
        cfg.tei_url,
        normalize_local_service_url("http://axon-tei:80".to_string())
    );
}

#[allow(unsafe_code)]
#[test]
fn into_config_errors_when_qdrant_url_missing() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        env::remove_var("QDRANT_URL");
    }

    let cli = Cli::parse_from(["axon", "--tei-url", "http://127.0.0.1:52000", "status"]);
    let err = into_config(cli).unwrap_err();
    assert!(
        err.contains("QDRANT_URL"),
        "expected QDRANT_URL error, got: {err}"
    );
}

#[allow(unsafe_code)]
#[test]
fn into_config_errors_when_tei_url_missing() {
    let _guard = ENV_LOCK.lock().unwrap();
    let orig_tei_url = env::var("TEI_URL").ok();
    unsafe {
        env::remove_var("TEI_URL");
    }

    let cli = Cli::parse_from(["axon", "--qdrant-url", "http://127.0.0.1:53333", "status"]);
    let err = into_config(cli).unwrap_err();
    assert!(
        err.contains("TEI_URL"),
        "expected TEI_URL error, got: {err}"
    );

    unsafe {
        if let Some(val) = orig_tei_url {
            env::set_var("TEI_URL", val);
        }
    }
}

#[allow(unsafe_code)]
#[test]
fn into_config_reads_acp_ws_url_from_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        env::set_var("AXON_ACP_WS_URL", "https://axon.example.com:49000");
    }
    let cli = Cli::parse_from([
        "axon",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "--tei-url",
        "http://127.0.0.1:52000",
        "status",
    ]);
    let cfg = into_config(cli).expect("status config should parse");
    assert_eq!(
        cfg.acp_ws_url.as_deref(),
        Some("https://axon.example.com:49000"),
        "acp_ws_url should be populated from AXON_ACP_WS_URL"
    );
    unsafe {
        env::remove_var("AXON_ACP_WS_URL");
    }
}

#[allow(unsafe_code)]
#[test]
fn into_config_reads_acp_ws_token_from_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        env::set_var("AXON_ACP_WS_TOKEN", "supersecret");
    }
    let cli = Cli::parse_from([
        "axon",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "--tei-url",
        "http://127.0.0.1:52000",
        "status",
    ]);
    let cfg = into_config(cli).expect("status config should parse");
    assert_eq!(
        cfg.acp_ws_token.as_deref(),
        Some("supersecret"),
        "acp_ws_token should be populated from AXON_ACP_WS_TOKEN"
    );
    unsafe {
        env::remove_var("AXON_ACP_WS_TOKEN");
    }
}
