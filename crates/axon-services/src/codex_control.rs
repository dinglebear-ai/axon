//! Services-first facade for the dedicated Codex app-server control plane.

use axon_codex::api::{ControlAction, WritePolicy, account_summary};
use axon_codex::control::{ControlConfig, ControlRuntime, ControlStatus};
use axon_codex::events::{EventCursor, RecordedEvent};
use axon_codex::operations::{ControlOperation, OperationIntent, OperationStore};
use axon_codex::protocol::RuntimeEpoch;
use axon_codex::transport::ControlTransport;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexControlSnapshot {
    pub status: ControlStatus,
    pub account: Value,
    pub models: Value,
    pub config: Value,
    pub mcp_servers: Value,
    pub plugins: Value,
    pub skills: Value,
    pub hooks: Value,
    pub apps: Value,
}

pub struct CodexControlService {
    runtime: Arc<ControlRuntime>,
    transport: Mutex<Option<Arc<ControlTransport>>>,
    operations: OperationStore,
    policy: WritePolicy,
}

impl CodexControlService {
    pub fn new(
        config: ControlConfig,
        policy: WritePolicy,
        database: &Path,
    ) -> Result<Self, String> {
        Ok(Self {
            runtime: Arc::new(ControlRuntime::new(config)?),
            transport: Mutex::new(None),
            operations: OperationStore::open(database)?,
            policy,
        })
    }

    pub async fn start(&self, config: &ControlConfig, epoch: RuntimeEpoch) -> Result<(), String> {
        let transport = Arc::new(ControlTransport::start(config, epoch).await?);
        *self.transport.lock().await = Some(transport);
        Ok(())
    }

    pub fn status(&self) -> ControlStatus {
        self.runtime.status()
    }

    pub async fn snapshot(&self) -> Result<CodexControlSnapshot, String> {
        let transport = self.transport().await?;
        self.runtime.with_read(|| async {
            let (account, models, config, mcp_servers, plugins, skills, hooks, apps) = tokio::join!(
                transport.request(ControlAction::AccountRead.method(), json!({"refreshToken": false})),
                transport.request(ControlAction::ModelsList.method(), json!({})),
                transport.request(ControlAction::ConfigRead.method(), json!({"includeLayers": true})),
                transport.request(ControlAction::McpServersList.method(), json!({})),
                transport.request(ControlAction::PluginsList.method(), json!({})),
                transport.request(ControlAction::SkillsList.method(), json!({})),
                transport.request(ControlAction::HooksList.method(), json!({})),
                transport.request(ControlAction::AppsList.method(), json!({})),
            );
            let account = serde_json::to_value(account_summary(&account?)).map_err(|error| error.to_string())?;
            Ok(CodexControlSnapshot { status: self.runtime.status(), account, models: models?, config: config?, mcp_servers: mcp_servers?, plugins: plugins?, skills: skills?, hooks: hooks?, apps: apps? })
        }).await
    }

    pub async fn read(&self, action: ControlAction, params: Value) -> Result<Value, String> {
        if action.is_mutation() {
            return Err("mutation action sent to read path".to_string());
        }
        let transport = self.transport().await?;
        self.runtime
            .with_read(|| transport.request(action.method(), params))
            .await
    }

    pub fn create_operation(&self, intent: &OperationIntent) -> Result<ControlOperation, String> {
        self.policy
            .authorize(&action_from_method(&intent.method)?)?;
        self.operations.create(intent)
    }

    pub fn approve_operation(&self, id: i64, approver: &str) -> Result<String, String> {
        self.operations.approve(id, approver)
    }

    pub async fn execute_operation(
        &self,
        id: i64,
        capability: &str,
        action: ControlAction,
        params: Value,
        revision: Option<&str>,
        home_identity: &str,
        policy_version: &str,
    ) -> Result<Value, String> {
        self.policy.authorize(&action)?;
        self.operations
            .begin_execution(id, capability, revision, home_identity, policy_version)?;
        let transport = self.transport().await?;
        let result = self
            .runtime
            .with_mutation(|| transport.request(action.method(), params))
            .await;
        match result {
            Ok(value) => {
                let post_revision = value
                    .get("revision")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                self.operations.reconcile(id, post_revision)?;
                Ok(value)
            }
            Err(error) => {
                self.operations.fail_ambiguous(id, &error)?;
                Err(error)
            }
        }
    }

    pub async fn events_after(
        &self,
        cursor: Option<EventCursor>,
        limit: usize,
    ) -> Result<Vec<RecordedEvent>, String> {
        self.transport().await?.events_after(cursor, limit)
    }

    async fn transport(&self) -> Result<Arc<ControlTransport>, String> {
        self.transport
            .lock()
            .await
            .clone()
            .ok_or_else(|| "Codex control runtime is not started".to_string())
    }
}

fn action_from_method(method: &str) -> Result<ControlAction, String> {
    [
        ControlAction::AccountLoginStart,
        ControlAction::AccountLoginCancel,
        ControlAction::AccountLogout,
        ControlAction::ConfigValueWrite,
        ControlAction::ConfigBatchWrite,
        ControlAction::McpServerReload,
        ControlAction::McpServerOauthLogin,
        ControlAction::PluginInstall,
        ControlAction::PluginUninstall,
        ControlAction::SkillConfigWrite,
        ControlAction::ExternalAgentConfigImport,
    ]
    .into_iter()
    .find(|action| action.method() == method)
    .ok_or_else(|| format!("unsupported Codex control mutation: {method}"))
}
