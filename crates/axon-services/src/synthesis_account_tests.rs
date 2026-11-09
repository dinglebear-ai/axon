use super::*;

#[test]
fn device_login_accepts_only_pinned_device_shape_and_approved_verification_origin() {
    let response = json!({"type":"chatgptDeviceCode", "loginId":"login-1",
        "userCode":"ABCD-EFGH", "verificationUrl":"https://auth.openai.com/codex/device",
        "accessToken":"must-never-appear"});
    let status = parse_device_login(&response).unwrap();
    let serialized = serde_json::to_string(&status).unwrap();
    assert!(!serialized.contains("must-never-appear"));
    assert_eq!(status.login_id.as_deref(), Some("login-1"));
    assert_eq!(status.expires_in_seconds, Some(600));
    for url in [
        "http://auth.openai.com/codex/device",
        "https://evil.test/device",
        "https://auth.openai.com/codex/device?token=secret",
    ] {
        let mut wrong = response.clone();
        wrong["verificationUrl"] = json!(url);
        assert!(parse_device_login(&wrong).is_err());
    }
    let mut wrong = response;
    wrong["type"] = json!("chatgptAuthTokens");
    assert!(parse_device_login(&wrong).is_err());
}

#[test]
fn account_status_is_masked_and_drops_unknown_auth_fields() {
    let projected = summary(
        &json!({"account":{"type":"chatgpt", "email":"alice@example.test",
        "planType":"pro", "accessToken":"secret", "id":"private-account-id"}}),
    );
    assert_eq!(
        projected,
        json!({"signed_in":true,"account_type":"chatgpt","email_hint":"a***@example.test","plan":"pro"})
    );
    assert_eq!(summary(&json!({"account":null}))["signed_in"], false);
    assert_eq!(
        summary(&json!({"requiresOpenaiAuth":true}))["signed_in"],
        false
    );
}

#[test]
fn account_writes_require_explicit_persistent_home_configuration() {
    let mut backend = LlmBackendConfig::default();
    assert!(
        account_config(&backend)
            .unwrap_err()
            .contains("unsupported")
    );
    backend.codex_load_user_config = true;
    backend.kind = axon_core::llm::LlmBackendKind::CodexAppServer;
    assert!(
        account_config(&backend)
            .unwrap_err()
            .contains("home_required")
    );
}

#[tokio::test]
async fn start_requires_intent_before_mutation_and_cancel_rejects_unknown_login() {
    let service = SynthesisAccountService {
        config: Err("unconfigured".into()),
        state: Arc::new(Mutex::new(State {
            status: AccountStatus {
                account: summary(&json!({"account":null})),
                login: login_status("idle"),
                rate_limits: None,
                error: None,
            },
            cancel: None,
            active: false,
        })),
    };
    assert!(
        service
            .start(false)
            .await
            .err()
            .unwrap()
            .contains("confirmation_required")
    );
    assert!(
        service
            .cancel("unrelated-id")
            .await
            .err()
            .unwrap()
            .contains("login_unknown")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn device_session_excludes_generation_preserves_account_and_cancels_exact_login() {
    use std::os::unix::fs::PermissionsExt;
    // The transport rejects world-writable ancestors, so use the owned checkout,
    // not /tmp, for this test's short-lived fixture binary and canonical home.
    let dir = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let binary = dir.path().join("fake-codex");
    std::fs::write(&binary, r#"#!/usr/bin/python3
import json, sys, pathlib, time
email = 'old@example.test'
for line in sys.stdin:
    req = json.loads(line)
    if 'id' not in req: continue
    method = req['method']
    result = {}
    if method == 'account/read':
        if pathlib.Path(__file__).with_name('delay-read').exists():
            pathlib.Path(__file__).with_name('read-started').touch()
            time.sleep(0.5)
        result = {'account': {'type': 'chatgpt', 'email': email, 'planType': 'pro'}}
    elif method == 'account/login/start':
        result = {'type': 'chatgptDeviceCode', 'loginId': 'test-login', 'userCode': 'ABCD-EFGH', 'verificationUrl': 'https://auth.openai.com/codex/device'}
    elif method == 'account/login/cancel':
        result = {'status': 'canceled'}
    elif method == 'account/logout':
        raise RuntimeError('logout must never be called')
    print(json.dumps({'id': req['id'], 'result': result}), flush=True)
    if method == 'account/login/start' and pathlib.Path(__file__).with_name('complete-next').exists():
        email = 'new@example.test'
        print(json.dumps({'method': 'account/login/completed', 'params': {'loginId': 'test-login', 'success': True}}), flush=True)
"#).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    let service = SynthesisAccountService {
        config: Ok(ControlConfig {
            enabled: true,
            codex_binary: binary,
            control_home: dir.path().to_path_buf(),
            request_timeout: Duration::from_secs(2),
            read_concurrency: 1,
            max_restart_backoff: Duration::from_secs(1),
        }),
        state: Arc::new(Mutex::new(State {
            status: AccountStatus {
                account: summary(&json!({"account":null})),
                login: login_status("idle"),
                rate_limits: None,
                error: None,
            },
            cancel: None,
            active: false,
        })),
    };
    let pending = service.start(true).await.unwrap();
    assert_eq!(pending.account["email_hint"], "o***@example.test");
    assert_eq!(pending.login.state, "pending");
    assert!(
        axon_llm::runtime::codex_app_server::account_gate::acquire()
            .await
            .is_err()
    );
    assert!(service.start(true).await.err().unwrap().contains("busy"));
    assert!(service.cancel("different-login").await.is_err());
    assert_eq!(service.status().await.unwrap().login.state, "pending");
    assert_eq!(
        service.cancel("test-login").await.unwrap().login.state,
        "cancelling"
    );
    let done = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let state = service.state.lock().await;
            if !state.active {
                break state.status.clone();
            }
            drop(state);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(done.login.state, "cancelled");
    assert!(done.login.user_code.is_none());
    assert_eq!(done.account["email_hint"], "o***@example.test");
    assert!(
        axon_llm::runtime::codex_app_server::account_gate::acquire()
            .await
            .is_ok()
    );
    std::fs::write(dir.path().join("complete-next"), "complete").unwrap();
    service.start(true).await.unwrap();
    let completed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let state = service.state.lock().await;
            if !state.active {
                break state.status.clone();
            }
            drop(state);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(completed.login.state, "completed");
    assert!(completed.login.user_code.is_none());
    assert_eq!(completed.account["email_hint"], "n***@example.test");
    // A dropped HTTP waiter must not cancel the owner or release canonical auth.
    std::fs::write(dir.path().join("delay-read"), "delay").unwrap();
    let owned = service.clone();
    let waiter = tokio::spawn(async move { owned.status().await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while !dir.path().join("read-started").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    waiter.abort();
    let _ = waiter.await;
    assert!(
        axon_llm::runtime::codex_app_server::account_gate::acquire()
            .await
            .is_err()
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(guard) = axon_llm::runtime::codex_app_server::account_gate::acquire().await {
                drop(guard);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    assert!(
        axon_llm::runtime::codex_app_server::account_gate::acquire()
            .await
            .is_ok()
    );
}

#[test]
fn quota_projection_omits_identity_credit_ids_and_untrusted_banners() {
    let quota = quota_summary(json!({"accountId":"private", "ordinaryUsageAllowed":false,
        "rateLimitUpsell":{"html":"untrusted"}, "rateLimits":{"limitId":"private-limit",
        "primary":{"usedPercent":100,"resetsAt":123,"accountId":"private-nested"},
        "planType":"pro", "rateLimitReachedType":"rate_limit_reached"}}));
    assert_eq!(quota["ordinaryUsageAllowed"], false);
    assert_eq!(quota["rateLimits"]["primary"]["usedPercent"], 100);
    assert!(!quota.to_string().contains("private"));
    assert!(!quota.to_string().contains("untrusted"));
}
