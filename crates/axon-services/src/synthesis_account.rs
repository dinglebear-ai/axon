//! Explicit ownership of the synthesis account, independent of trusted control.
//! Device login temporarily pauses synthesis; credentials stay in canonical home.
use axon_codex::{
    api::account_summary,
    control::ControlConfig,
    events::{EventKind, sanitize_value},
    protocol::RuntimeEpoch,
    transport::ControlTransport,
};
use axon_core::{config::Config, llm::LlmBackendConfig};
use serde::Serialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::sync::{Mutex, oneshot};
use utoipa::ToSchema;

const LOGIN_TTL: Duration = Duration::from_secs(600);

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct LoginStatus {
    pub state: String,
    pub login_id: Option<String>,
    pub user_code: Option<String>,
    pub verification_url: Option<String>,
    pub expires_in_seconds: Option<u64>,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct AccountStatus {
    /// Masked summary of the account used for synthesis, never raw auth.
    pub account: Value,
    pub login: LoginStatus,
    pub rate_limits: Option<Value>,
    pub error: Option<String>,
}

struct State {
    status: AccountStatus,
    cancel: Option<oneshot::Sender<()>>,
    active: bool,
}

#[derive(Clone)]
pub struct SynthesisAccountService {
    config: Result<ControlConfig, String>,
    state: Arc<Mutex<State>>,
}

fn login_status(state: &str) -> LoginStatus {
    LoginStatus {
        state: state.into(),
        login_id: None,
        user_code: None,
        verification_url: None,
        expires_in_seconds: None,
    }
}

fn summary(value: &Value) -> Value {
    // account/read may omit account entirely when signed out. Never interpret
    // its requiresOpenaiAuth envelope as evidence of an authenticated account.
    let normalized = json!({"account": value.get("account").unwrap_or(&Value::Null)});
    serde_json::to_value(account_summary(&normalized)).expect("account summary serializes")
}

impl SynthesisAccountService {
    pub fn from_config(cfg: &Config) -> Arc<Self> {
        let backend = LlmBackendConfig::from_config(cfg);
        let config = account_config(&backend);
        Arc::new(Self {
            config,
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
        })
    }

    async fn connect(&self) -> Result<ControlTransport, String> {
        let config = self.config.as_ref().map_err(Clone::clone)?;
        ControlTransport::start(config, RuntimeEpoch(1)).await
            .map_err(|error| {
                if error.starts_with("codex_control_cleanup_failed:") {
                    "synthesis_account_cleanup_failed: initialization child could not be reaped; restart service before generating".into()
                } else {
                    "synthesis_account_unavailable: cannot start account session; verify dedicated Codex binary/home permissions".into()
                }
            })
    }

    // An owned task keeps authentication exclusive even if the HTTP caller disconnects.
    pub async fn status(&self) -> Result<AccountStatus, String> {
        let service = self.clone();
        tokio::spawn(async move { service.status_owned().await })
            .await
            .map_err(|_| "synthesis_account_owner_failed: account owner failed".to_string())?
    }

    async fn status_owned(&self) -> Result<AccountStatus, String> {
        let mut state = self.state.lock().await;
        if state.active {
            return Ok(state.status.clone());
        }
        let gate = axon_llm::runtime::codex_app_server::account_gate::acquire().await?;
        let (transport, gate) = self.connect_owned(gate).await?;
        let result = read_status(&transport, state.status.login.clone()).await;
        stop_owned(transport, gate).await?;
        state.status = result?;
        Ok(state.status.clone())
    }

    // An owned task keeps authentication exclusive even if the HTTP caller disconnects.
    pub async fn start(&self, switch_account: bool) -> Result<AccountStatus, String> {
        let service = self.clone();
        tokio::spawn(async move { service.start_owned(switch_account).await })
            .await
            .map_err(|_| "synthesis_account_owner_failed: account owner failed".to_string())?
    }

    async fn start_owned(&self, switch_account: bool) -> Result<AccountStatus, String> {
        if !switch_account {
            return Err(
                "synthesis_account_confirmation_required: explicitly confirm account switching"
                    .into(),
            );
        }
        let mut state = self.state.lock().await;
        if state.active {
            return Err(
                "synthesis_account_busy: a device sign-in is already pending; finish or cancel it"
                    .into(),
            );
        }
        let gate = axon_llm::runtime::codex_app_server::account_gate::acquire().await?;
        let (transport, gate) = self.connect_owned(gate).await?;
        // Subscribe before start so an immediate completion cannot be lost.
        let mut events = transport.subscribe();
        state.status = match read_status(&transport, login_status("idle")).await {
            Ok(status) => status,
            Err(error) => {
                stop_owned(transport, gate).await?;
                return Err(error);
            }
        };
        let response = match transport
            .request("account/login/start", json!({"type":"chatgptDeviceCode"}))
            .await
        {
            Ok(response) => response,
            Err(_) => {
                stop_owned(transport, gate).await?;
                return Err("synthesis_account_login_start_failed: start was not acknowledged; inspect account status before retrying".into());
            }
        };
        let login = match parse_device_login(&response) {
            Ok(login) => login,
            Err(error) => {
                stop_owned(transport, gate).await?;
                return Err(error);
            }
        };
        let id = login.login_id.clone().expect("validated login id");
        state.status.login = login;
        state.status.error = None;
        let (cancel, mut cancelled) = oneshot::channel();
        state.cancel = Some(cancel);
        state.active = true;
        let shared = self.state.clone();
        tokio::spawn(async move {
            let _gate = gate;
            let timeout = tokio::time::sleep(LOGIN_TTL);
            tokio::pin!(timeout);
            let outcome = loop {
                tokio::select! {
                    _ = &mut cancelled => break "cancelled",
                    _ = &mut timeout => break "expired",
                    event = events.recv() => match event {
                        Ok(event) => match event.event {
                            EventKind::Notification { method, params }
                                if method == "account/login/completed" && params.get("loginId").and_then(Value::as_str) == Some(&id) => {
                                    break if params.get("success").and_then(Value::as_bool) == Some(true) { "completed" } else { "failed" };
                                }
                            EventKind::Exited => break "failed",
                            _ => {}
                        },
                        Err(_) => break "failed",
                    }
                }
            };
            if outcome != "completed" {
                let _ = transport
                    .request("account/login/cancel", json!({"loginId":id}))
                    .await;
            }
            let result = read_status(&transport, login_status(outcome)).await;
            let stopped = transport.stop().await;
            let mut state = shared.lock().await;
            if let Ok(status) = result {
                state.status = status;
            } else {
                state.status.login = login_status(outcome);
            }
            if outcome == "failed" {
                state.status.error = Some("synthesis_account_login_failed: device authorization failed; existing account may remain active, refresh status before retrying".into());
            }
            if stopped.is_err() {
                state.status.error = Some("synthesis_account_cleanup_failed: account session could not be reaped; restart the service before generating".into());
                std::mem::forget(_gate);
            }
            state.cancel = None;
            state.active = false;
        });
        Ok(state.status.clone())
    }

    async fn connect_owned(
        &self,
        gate: tokio::sync::OwnedRwLockWriteGuard<()>,
    ) -> Result<(ControlTransport, tokio::sync::OwnedRwLockWriteGuard<()>), String> {
        match self.connect().await {
            Ok(transport) => Ok((transport, gate)),
            Err(error) => {
                if error.starts_with("synthesis_account_cleanup_failed:") {
                    std::mem::forget(gate);
                }
                Err(error)
            }
        }
    }

    pub async fn cancel(&self, login_id: &str) -> Result<AccountStatus, String> {
        let mut state = self.state.lock().await;
        if state.status.login.login_id.as_deref() != Some(login_id) || !state.active {
            return Err("synthesis_account_login_unknown: sign-in identifier is no longer pending; refresh status".into());
        }
        if let Some(cancel) = state.cancel.take() {
            let _ = cancel.send(());
        }
        // Keep ownership visible until the background owner stops/reaps its session.
        state.status.login.state = "cancelling".into();
        state.status.login.user_code = None;
        Ok(state.status.clone())
    }
}

async fn stop_owned(
    transport: ControlTransport,
    gate: tokio::sync::OwnedRwLockWriteGuard<()>,
) -> Result<(), String> {
    if transport.stop().await.is_err() {
        // Preserve the existing fail-closed policy when child liveness is unknown.
        std::mem::forget(gate);
        return Err("synthesis_account_cleanup_failed: account session could not be reaped; restart service before generating".into());
    }
    Ok(())
}

fn account_config(backend: &LlmBackendConfig) -> Result<ControlConfig, String> {
    if !backend.codex_load_user_config
        || backend.kind != axon_core::llm::LlmBackendKind::CodexAppServer
    {
        return Err("synthesis_account_unsupported: account management requires dedicated persistent AXON_CODEX_HOME and AXON_CODEX_LOAD_USER_CONFIG=true".into());
    }
    // Require explicit ownership, never infer a workstation home for writes.
    let home = backend.codex_home.clone().ok_or_else(|| "synthesis_account_home_required: configure explicit AXON_CODEX_HOME before account management".to_string())?;
    let config = ControlConfig {
        enabled: true,
        codex_binary: backend.codex_cmd.clone().into(),
        control_home: home,
        request_timeout: Duration::from_secs(20),
        read_concurrency: 1,
        max_restart_backoff: Duration::from_secs(60),
    };
    axon_codex::control::validate_config(&config)?;
    Ok(config)
}

fn parse_device_login(value: &Value) -> Result<LoginStatus, String> {
    let string = |key| {
        value
            .get(key)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty() && v.len() <= 512)
            .map(str::to_owned)
    };
    let id = string("loginId");
    let code = string("userCode");
    let url = string("verificationUrl");
    if value.get("type").and_then(Value::as_str) != Some("chatgptDeviceCode")
        || id.is_none()
        || code.is_none()
        || !url
            .as_deref()
            .is_some_and(|url| url == "https://auth.openai.com/codex/device")
    {
        return Err("synthesis_account_protocol_mismatch: device login response has an unsupported shape or verification URL; inspect pinned Codex version before retrying".into());
    }
    Ok(LoginStatus {
        state: "pending".into(),
        login_id: id,
        user_code: code,
        verification_url: url,
        expires_in_seconds: Some(LOGIN_TTL.as_secs()),
    })
}

async fn read_status(
    transport: &ControlTransport,
    login: LoginStatus,
) -> Result<AccountStatus, String> {
    let value = transport.request("account/read", json!({"refreshToken":false})).await
        .map_err(|_| "synthesis_account_read_failed: cannot read Codex account; verify authentication and retry status".to_string())?;
    let limits = transport
        .request("account/rateLimits/read", json!({}))
        .await;
    Ok(AccountStatus { account: summary(&value), login,
        error: limits.as_ref().err().map(|_| "synthesis_account_quota_unavailable: Codex did not return quota; authentication or provider limits may require attention".into()),
        rate_limits: limits.ok().map(quota_summary) })
}

// Quota payloads can contain account IDs, credit IDs and backend-owned banners.
// Return only the fields needed to explain quota state in the portal.
fn quota_summary(value: Value) -> Value {
    let source = value.get("rateLimits").unwrap_or(&Value::Null);
    let mut limits = serde_json::Map::new();
    for key in [
        "primary",
        "secondary",
        "planType",
        "rateLimitReachedType",
        "spendControlReached",
    ] {
        if let Some(value) = source.get(key) {
            if matches!(key, "primary" | "secondary") {
                let mut window = serde_json::Map::new();
                for field in ["usedPercent", "resetsAt", "windowDurationMins"] {
                    if let Some(value) = value.get(field) {
                        window.insert(field.into(), value.clone());
                    }
                }
                limits.insert(
                    key.into(),
                    if value.is_null() {
                        Value::Null
                    } else {
                        Value::Object(window)
                    },
                );
            } else {
                limits.insert(key.into(), value.clone());
            }
        }
    }
    sanitize_value(
        json!({"ordinaryUsageAllowed": value.get("ordinaryUsageAllowed"), "rateLimits": limits}),
    )
}

#[cfg(test)]
#[path = "synthesis_account_tests.rs"]
mod tests;
