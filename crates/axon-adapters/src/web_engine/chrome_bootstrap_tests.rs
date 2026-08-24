use super::*;
use crate::web_engine::engine::cdp_probe_skipped_in_docker;

fn chrome_cfg(remote_url: Option<&str>) -> Config {
    Config {
        render_mode: RenderMode::Chrome,
        chrome_remote_url: remote_url.map(str::to_string),
        chrome_bootstrap_timeout_ms: 200,
        chrome_bootstrap_retries: 0,
        ..Config::default()
    }
}

#[tokio::test]
async fn unset_remote_warns_without_marking_unreachable() {
    let outcome = bootstrap_chrome_runtime(&chrome_cfg(None)).await;

    assert!(!outcome.remote_ready);
    assert!(!outcome.remote_unreachable);
    assert!(outcome.resolved_ws_url.is_none());
    assert!(outcome.warnings.iter().any(|w| w.contains("unset")));
}

#[tokio::test]
async fn dead_remote_is_marked_unreachable_after_retries() {
    // Skip inside Docker: the probe never runs there and the outcome
    // deliberately stays neutral.
    if cdp_probe_skipped_in_docker() {
        return;
    }

    // Port 9 (discard) on loopback is not listening in any test environment.
    let outcome = bootstrap_chrome_runtime(&chrome_cfg(Some("http://127.0.0.1:9"))).await;

    assert!(!outcome.remote_ready);
    assert!(outcome.remote_unreachable);
    assert!(outcome.resolved_ws_url.is_none());
    assert!(outcome.warnings.iter().any(|w| w.contains("unreachable")));
}

#[tokio::test]
async fn ws_url_short_circuits_the_probe() {
    let ws = "ws://127.0.0.1:1/devtools/browser/test";
    let outcome = bootstrap_chrome_runtime(&chrome_cfg(Some(ws))).await;

    assert!(outcome.remote_ready);
    assert!(!outcome.remote_unreachable);
    assert_eq!(outcome.resolved_ws_url.as_deref(), Some(ws));
}

#[test]
fn apply_outcome_installs_the_resolved_ws_url() {
    let mut cfg = chrome_cfg(Some("http://127.0.0.1:6000"));
    let outcome = ChromeBootstrapOutcome {
        remote_ready: true,
        resolved_ws_url: Some("ws://127.0.0.1:9222/devtools/browser/x".to_string()),
        remote_unreachable: false,
        warnings: Vec::new(),
    };

    apply_bootstrap_outcome(&mut cfg, &outcome);

    assert_eq!(
        cfg.chrome_remote_url.as_deref(),
        Some("ws://127.0.0.1:9222/devtools/browser/x")
    );
}

#[test]
fn apply_outcome_clears_an_unreachable_remote() {
    let mut cfg = chrome_cfg(Some("http://127.0.0.1:6000"));
    let outcome = ChromeBootstrapOutcome {
        remote_ready: false,
        resolved_ws_url: None,
        remote_unreachable: true,
        warnings: Vec::new(),
    };

    apply_bootstrap_outcome(&mut cfg, &outcome);

    assert!(cfg.chrome_remote_url.is_none());
}

#[test]
fn apply_outcome_leaves_config_untouched_when_the_probe_was_skipped() {
    let mut cfg = chrome_cfg(Some("http://axon-chrome:6000"));
    let outcome = ChromeBootstrapOutcome {
        remote_ready: false,
        resolved_ws_url: None,
        remote_unreachable: false,
        warnings: Vec::new(),
    };

    apply_bootstrap_outcome(&mut cfg, &outcome);

    assert_eq!(
        cfg.chrome_remote_url.as_deref(),
        Some("http://axon-chrome:6000")
    );
}
