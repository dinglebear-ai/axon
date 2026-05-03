use std::sync::Arc;

use agent_client_protocol::{
    Agent, ClientSideConnection, SessionId, SetSessionConfigOptionRequest, SetSessionModeRequest,
};
use tokio::sync::mpsc;

use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::types::{AcpBridgeEvent, AcpConfigOption};

use super::super::bridge::AcpRuntimeState;

pub(super) async fn apply_requested_model_before_prompt(
    conn: &ClientSideConnection,
    session_id: &SessionId,
    runtime_state: &Arc<AcpRuntimeState>,
    requested_model: Option<&str>,
    service_tx: &Option<mpsc::Sender<ServiceEvent>>,
) -> Result<(), String> {
    let Some(requested) = requested_model.map(str::trim).filter(|m| !m.is_empty()) else {
        return Ok(());
    };

    let established = runtime_state.established_model.borrow().clone();
    if established.as_deref() == Some(requested) {
        return Ok(());
    }

    let known_options = runtime_state.config_options.borrow().clone();
    let (option_id, value_allowed) = resolve_model_option_for_request(&known_options, requested);
    if !value_allowed {
        tracing::warn!(
            session_id = %session_id.0,
            requested_model = %requested,
            option_id = %option_id,
            "acp: requested model rejected by adapter config options"
        );
        let msg = format!(
            "ACP runtime: requested model '{requested}' is not in ACP config options; keeping current model"
        );
        crate::crates::core::logging::log_warn(&msg);
        emit(
            service_tx,
            ServiceEvent::Log {
                level: LogLevel::Warn,
                message: msg,
            },
        )
        .await;
        return Ok(());
    }

    let msg = format!(
        "ACP runtime: applying model change mid-session (option_id={option_id}, value={requested})"
    );
    crate::crates::core::logging::log_info(&msg);
    emit(
        service_tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: msg,
        },
    )
    .await;

    let set_resp = conn
        .set_session_config_option(SetSessionConfigOptionRequest::new(
            session_id.clone(),
            option_id,
            requested,
        ))
        .await
        .map_err(|err| {
            tracing::warn!(
                session_id = %session_id.0,
                requested_model = %requested,
                error = %err,
                "acp: failed to apply requested model before prompt"
            );
            format!("set_session_config_option failed: {err}")
        })?;

    emit_config_options_update(
        runtime_state,
        session_id,
        service_tx,
        set_resp.config_options,
    )
    .await;
    *runtime_state.established_model.borrow_mut() = Some(requested.to_string());
    Ok(())
}

pub(super) fn resolve_model_option_for_request(
    options: &[AcpConfigOption],
    requested_model: &str,
) -> (String, bool) {
    let model_option = options
        .iter()
        .find(|opt| opt.category.as_deref() == Some("model"));
    if let Some(opt) = model_option {
        let allowed = opt.options.iter().any(|o| o.value == requested_model);
        return (opt.id.clone(), allowed);
    }
    // Fallback for adapters that do not provide config options but still accept
    // the conventional `model` config ID.
    ("model".to_string(), true)
}

pub(super) async fn apply_requested_mode_before_prompt(
    conn: &ClientSideConnection,
    session_id: &SessionId,
    runtime_state: &Arc<AcpRuntimeState>,
    requested_mode: Option<&str>,
    service_tx: &Option<mpsc::Sender<ServiceEvent>>,
) -> Result<(), String> {
    let Some(requested) = requested_mode.map(str::trim).filter(|m| !m.is_empty()) else {
        return Ok(());
    };

    // Dedup: skip RPC if the adapter is already in the requested mode.
    let current = runtime_state.current_mode.borrow().clone();
    if current.as_deref() == Some(requested) {
        return Ok(());
    }

    let known_options = runtime_state.config_options.borrow().clone();
    let (_option_id, value_allowed) = resolve_mode_option_for_request(&known_options, requested);
    if !value_allowed {
        tracing::warn!(
            session_id = %session_id.0,
            requested_mode = %requested,
            "acp: requested session mode rejected by adapter config options"
        );
        let msg = format!(
            "ACP runtime: requested session_mode '{requested}' is not in ACP mode options; keeping current value"
        );
        crate::crates::core::logging::log_warn(&msg);
        emit(
            service_tx,
            ServiceEvent::Log {
                level: LogLevel::Warn,
                message: msg,
            },
        )
        .await;
        return Ok(());
    }

    let msg = format!(
        "ACP runtime: applying session_mode change (mode={requested}, session_id={})",
        session_id.0
    );
    crate::crates::core::logging::log_info(&msg);
    emit(
        service_tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: msg,
        },
    )
    .await;

    conn.set_session_mode(SetSessionModeRequest::new(
        session_id.clone(),
        requested.to_string(),
    ))
    .await
    .map_err(|err| {
        tracing::warn!(
            session_id = %session_id.0,
            requested_mode = %requested,
            error = %err,
            "acp: failed to apply requested session mode before prompt"
        );
        format!("set_session_mode failed: {err}")
    })?;

    // Update tracked mode so subsequent turns with the same mode are no-ops.
    *runtime_state.current_mode.borrow_mut() = Some(requested.to_string());
    Ok(())
}

pub(super) fn resolve_mode_option_for_request(
    options: &[AcpConfigOption],
    requested_mode: &str,
) -> (String, bool) {
    let canonical_requested = requested_mode.trim().replace('-', "_").to_lowercase();
    let mode_option = options
        .iter()
        .find(|opt| opt.category.as_deref() == Some("mode"));
    if let Some(opt) = mode_option {
        let direct_allowed = opt.options.iter().any(|o| o.value == requested_mode);
        if direct_allowed {
            return (opt.id.clone(), true);
        }
        let alias_allowed = opt.options.iter().any(|o| {
            o.value.trim().replace('-', "_").to_lowercase() == canonical_requested
                || o.name.trim().replace('-', "_").to_lowercase() == canonical_requested
        });
        return (opt.id.clone(), alias_allowed);
    }
    // Conservative fallback: no known mode option means do not guess/apply.
    ("".to_string(), false)
}

async fn emit_config_options_update(
    runtime_state: &Arc<AcpRuntimeState>,
    session_id: &SessionId,
    service_tx: &Option<mpsc::Sender<ServiceEvent>>,
    raw_options: Vec<agent_client_protocol::SessionConfigOption>,
) {
    let updated = super::super::mapping::map_config_options(&raw_options);
    if updated.is_empty() {
        return;
    }
    tracing::debug!(
        session_id = %session_id.0,
        option_count = updated.len(),
        "acp: persistent session config options updated"
    );
    *runtime_state.config_options.borrow_mut() = updated.clone();
    emit(
        service_tx,
        ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::ConfigOptionsUpdate {
                session_id: session_id.0.to_string(),
                config_options: updated,
            },
        },
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::services::types::AcpConfigSelectValue;

    #[test]
    fn resolve_model_option_uses_model_category() {
        let options = vec![AcpConfigOption {
            id: "model_select".to_string(),
            name: "Model".to_string(),
            description: None,
            category: Some("model".to_string()),
            current_value: "sonnet".to_string(),
            options: vec![
                AcpConfigSelectValue {
                    value: "sonnet".to_string(),
                    name: "Sonnet".to_string(),
                    description: None,
                },
                AcpConfigSelectValue {
                    value: "opus".to_string(),
                    name: "Opus".to_string(),
                    description: None,
                },
            ],
        }];

        let (id, allowed) = resolve_model_option_for_request(&options, "opus");
        assert_eq!(id, "model_select");
        assert!(allowed);
    }

    #[test]
    fn resolve_model_option_rejects_unknown_value_when_options_known() {
        let options = vec![AcpConfigOption {
            id: "model".to_string(),
            name: "Model".to_string(),
            description: None,
            category: Some("model".to_string()),
            current_value: "sonnet".to_string(),
            options: vec![AcpConfigSelectValue {
                value: "sonnet".to_string(),
                name: "Sonnet".to_string(),
                description: None,
            }],
        }];

        let (_id, allowed) = resolve_model_option_for_request(&options, "not-valid");
        assert!(!allowed);
    }

    #[test]
    fn resolve_model_option_falls_back_to_default_model_id() {
        let options: Vec<AcpConfigOption> = Vec::new();
        let (id, allowed) = resolve_model_option_for_request(&options, "anything");
        assert_eq!(id, "model");
        assert!(allowed);
    }

    #[test]
    fn resolve_mode_option_uses_mode_category() {
        let options = vec![AcpConfigOption {
            id: "approval_mode".to_string(),
            name: "Approval Mode".to_string(),
            description: None,
            category: Some("mode".to_string()),
            current_value: "default".to_string(),
            options: vec![AcpConfigSelectValue {
                value: "default".to_string(),
                name: "Default".to_string(),
                description: None,
            }],
        }];

        let (id, allowed) = resolve_mode_option_for_request(&options, "default");
        assert_eq!(id, "approval_mode");
        assert!(allowed);
    }

    #[test]
    fn resolve_mode_option_returns_not_allowed_when_missing() {
        let options: Vec<AcpConfigOption> = Vec::new();
        let (id, allowed) = resolve_mode_option_for_request(&options, "default");
        assert_eq!(id, "");
        assert!(!allowed);
    }

    #[test]
    fn resolve_mode_option_accepts_hyphen_underscore_alias() {
        let options = vec![AcpConfigOption {
            id: "approval_mode".to_string(),
            name: "Approval Mode".to_string(),
            description: None,
            category: Some("mode".to_string()),
            current_value: "accept_edits".to_string(),
            options: vec![AcpConfigSelectValue {
                value: "accept_edits".to_string(),
                name: "Accept edits".to_string(),
                description: None,
            }],
        }];

        let (id, allowed) = resolve_mode_option_for_request(&options, "accept-edits");
        assert_eq!(id, "approval_mode");
        assert!(allowed);
    }
}
