use super::AxonMcpServer;
use super::common::{invalid_params, logged_internal_error};
use crate::schema::{AxonToolResponse, CodexRequest, CodexSubaction};
use axon_services::codex_control::{ControlAction, EventCursor, MutationAction, OperationIntent};
use rmcp::ErrorData;
use serde_json::{Value, json};

impl AxonMcpServer {
    pub(super) async fn handle_codex(
        &self,
        req: CodexRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let service = self
            .codex_control_service()
            .await
            .map_err(|error| invalid_params(format!("codex.initialize: {error}")))?;
        let subaction = format!("{:?}", req.subaction).to_ascii_lowercase();
        let data = match req.subaction {
            CodexSubaction::Snapshot => {
                serde_json::to_value(service.snapshot().await.map_err(codex_error)?)
                    .map_err(|error| logged_internal_error("codex.snapshot", &error))?
            }
            CodexSubaction::Resource => {
                let resource = required(req.resource, "resource")?;
                let action = resource_action(&resource)?;
                service
                    .read(action, req.params.unwrap_or_else(|| json!({})))
                    .await
                    .map_err(codex_error)?
            }
            CodexSubaction::Events => {
                let cursor = match (req.cursor_boot_id, req.after_sequence) {
                    (Some(boot_id), Some(sequence)) => Some(EventCursor { boot_id, sequence }),
                    _ => None,
                };
                serde_json::to_value(
                    service
                        .events_after(cursor, req.limit.unwrap_or(100))
                        .await
                        .map_err(codex_error)?,
                )
                .map_err(|error| logged_internal_error("codex.events", &error))?
            }
            CodexSubaction::Operations => {
                serde_json::to_value(service.unfinished_operations().map_err(codex_error)?)
                    .map_err(|error| logged_internal_error("codex.operations", &error))?
            }
            CodexSubaction::Prepare => {
                let mutation = mutation_action(required(req.mutation_action, "mutation_action")?)?;
                let params = req
                    .params
                    .ok_or_else(|| invalid_params("codex prepare requires params"))?;
                let intent = OperationIntent {
                    actor: "mcp-admin".to_string(),
                    scope: "axon:admin".to_string(),
                    method: mutation.method().to_string(),
                    target_home_identity: String::new(),
                    runtime_boot_id: 0,
                    policy_version: String::new(),
                    expected_revision: None,
                    idempotency_key: required(req.idempotency_key, "idempotency_key")?,
                    redacted_request: params,
                };
                serde_json::to_value(
                    service
                        .create_operation(mutation, &intent)
                        .await
                        .map_err(codex_error)?,
                )
                .map_err(|error| logged_internal_error("codex.prepare", &error))?
            }
            CodexSubaction::Approve => json!({
                "approval_capability": service.approve_operation(required(req.operation_id, "operation_id")?, "mcp-admin").map_err(codex_error)?
            }),
            CodexSubaction::Execute => {
                let mutation = mutation_action(required(req.mutation_action, "mutation_action")?)?;
                service
                    .execute_operation(
                        required(req.operation_id, "operation_id")?,
                        &required(req.capability, "capability")?,
                        mutation,
                        req.params
                            .ok_or_else(|| invalid_params("codex execute requires params"))?,
                    )
                    .await
                    .map_err(codex_error)?
            }
            CodexSubaction::Reconcile => {
                service
                    .resolve_recovery(required(req.operation_id, "operation_id")?)
                    .await
                    .map_err(codex_error)?;
                json!({"reconciled": true})
            }
            CodexSubaction::Respond => {
                service
                    .respond_to_server_request(
                        required(req.boot_id, "boot_id")?,
                        required(req.request_id, "request_id")?,
                        required(req.approved, "approved")?,
                        req.params,
                    )
                    .await
                    .map_err(codex_error)?;
                json!({"responded": true})
            }
        };
        Ok(AxonToolResponse::ok("codex", &subaction, data))
    }
}

fn required<T>(value: Option<T>, name: &str) -> Result<T, ErrorData> {
    value.ok_or_else(|| invalid_params(format!("codex request requires {name}")))
}

fn mutation_action(value: String) -> Result<MutationAction, ErrorData> {
    serde_json::from_value(Value::String(value))
        .map_err(|error| invalid_params(format!("invalid Codex mutation action: {error}")))
}

fn resource_action(resource: &str) -> Result<ControlAction, ErrorData> {
    match resource {
        "account" => Ok(ControlAction::AccountRead),
        "models" => Ok(ControlAction::ModelsList),
        "config" => Ok(ControlAction::ConfigRead),
        "mcp_servers" => Ok(ControlAction::McpServersList),
        "mcp_resource" => Ok(ControlAction::McpServerResourceRead),
        "plugins" => Ok(ControlAction::PluginsList),
        "plugins_installed" => Ok(ControlAction::PluginsInstalled),
        "plugin_search" => Ok(ControlAction::PluginSearch),
        "plugin" => Ok(ControlAction::PluginRead),
        "plugin_skill" => Ok(ControlAction::PluginSkillRead),
        "plugin_shares" => Ok(ControlAction::PluginShareList),
        "skills" => Ok(ControlAction::SkillsList),
        "external_agent_config" => Ok(ControlAction::ExternalAgentConfigDetect),
        "external_agent_import_histories" => {
            Ok(ControlAction::ExternalAgentConfigImportReadHistories)
        }
        "hooks" => Ok(ControlAction::HooksList),
        "apps" => Ok(ControlAction::AppsList),
        "apps_installed" => Ok(ControlAction::AppsInstalled),
        "app" => Ok(ControlAction::AppRead),
        _ => Err(invalid_params(format!(
            "unknown Codex resource: {resource}"
        ))),
    }
}

fn codex_error(error: String) -> ErrorData {
    invalid_params(format!("Codex control request failed: {error}"))
}
