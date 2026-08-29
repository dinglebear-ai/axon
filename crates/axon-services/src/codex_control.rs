//! Services-first facade for the dedicated Codex app-server control plane.

use axon_codex::api::{ControlAction, WritePolicy, account_summary};
use axon_codex::control::{ControlConfig, ControlRuntime, ControlStatus, home_identity};
use axon_codex::events::{EventCursor, RecordedEvent};
use axon_codex::operations::{ControlOperation, OperationIntent, OperationStore};
use axon_codex::protocol::RuntimeEpoch;
use axon_codex::transport::ControlTransport;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
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
    config: ControlConfig,
    runtime: Arc<ControlRuntime>,
    transport: Mutex<Option<Arc<ControlTransport>>>,
    operations: OperationStore,
    policy: WritePolicy,
    home_identity: String,
    runtime_boot_id: AtomicU64,
    policy_version: String,
}

impl CodexControlService {
    pub fn new(
        config: ControlConfig,
        policy: WritePolicy,
        database: &Path,
    ) -> Result<Self, String> {
        let home_identity = home_identity(&config.control_home)?;
        let policy_version = format!(
            "v1:a{}c{}m{}p{}s{}i{}",
            policy.account as u8,
            policy.config as u8,
            policy.mcp as u8,
            policy.plugins as u8,
            policy.skills as u8,
            policy.imports as u8
        );
        Ok(Self {
            config: config.clone(),
            runtime: Arc::new(ControlRuntime::new(config)?),
            transport: Mutex::new(None),
            operations: OperationStore::open(database)?,
            policy,
            home_identity,
            runtime_boot_id: AtomicU64::new(0),
            policy_version,
        })
    }

    pub async fn start(&self, config: &ControlConfig, epoch: RuntimeEpoch) -> Result<(), String> {
        let transport = Arc::new(ControlTransport::start(config, epoch).await?);
        *self.transport.lock().await = Some(transport);
        self.runtime_boot_id.store(epoch.0, Ordering::Release);
        self.runtime.mark_ready();
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

    pub async fn create_operation(
        &self,
        intent: &OperationIntent,
    ) -> Result<ControlOperation, String> {
        self.policy
            .authorize(&action_from_method(&intent.method)?)?;
        self.transport().await?;
        let mut trusted_intent = intent.clone();
        trusted_intent.target_home_identity = self.home_identity.clone();
        trusted_intent.runtime_boot_id = self.runtime_boot_id.load(Ordering::Acquire);
        trusted_intent.policy_version = self.policy_version.clone();
        self.operations.create(&trusted_intent)
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
    ) -> Result<Value, String> {
        self.policy.authorize(&action)?;
        self.operations.begin_execution(
            id,
            capability,
            action.method(),
            &params,
            revision,
            &self.home_identity,
            self.runtime_boot_id.load(Ordering::Acquire),
            &self.policy_version,
        )?;
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
        let mut slot = self.transport.lock().await;
        if let Some(transport) = slot.as_ref() {
            return Ok(Arc::clone(transport));
        }
        let boot_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("system clock before epoch: {error}"))?
            .as_millis() as u64;
        let transport = match ControlTransport::start(&self.config, RuntimeEpoch(boot_id)).await {
            Ok(transport) => Arc::new(transport),
            Err(error) => {
                self.runtime.mark_degraded(error.clone());
                return Err(error);
            }
        };
        self.runtime_boot_id.store(boot_id, Ordering::Release);
        self.runtime.mark_ready();
        *slot = Some(Arc::clone(&transport));
        Ok(transport)
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
        ControlAction::MarketplaceAdd,
        ControlAction::MarketplaceRemove,
        ControlAction::MarketplaceUpgrade,
        ControlAction::SkillConfigWrite,
        ControlAction::ExternalAgentConfigImport,
    ]
    .into_iter()
    .find(|action| action.method() == method)
    .ok_or_else(|| format!("unsupported Codex control mutation: {method}"))
}
