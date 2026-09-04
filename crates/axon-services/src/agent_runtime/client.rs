use axon_api::agent::AgentToolProposal;
use axon_core::config::Config;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabbyContextReceipt {
    pub execution_context_id: String,
    pub actor: String,
    pub service: String,
    pub loadout_id: String,
    pub loadout_revision: u64,
    pub expires_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabbyExecutionReceipt {
    pub request_id: String,
    pub receipt_id: String,
    pub audit_id: String,
    pub status: String,
    pub tool_id: String,
    pub contract_hash: String,
    pub loadout_id: String,
    pub loadout_revision: u64,
    pub actor: String,
    pub service: String,
    pub execution_mode: String,
    pub llm_invocations: u8,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error_kind: Option<String>,
}

pub struct LabbyAgentClient {
    base: reqwest::Url,
    token: String,
    client: reqwest::Client,
}

impl LabbyAgentClient {
    pub fn from_config(cfg: &Config) -> anyhow::Result<Self> {
        let base = cfg
            .labby_url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("loadout_backend_missing"))?;
        let token = cfg
            .labby_service_token
            .clone()
            .ok_or_else(|| anyhow::anyhow!("loadout_auth_denied"))?;
        let url =
            reqwest::Url::parse(base).map_err(|_| anyhow::anyhow!("loadout_backend_invalid"))?;
        if url.scheme() != "https" && !matches!(url.host_str(), Some("127.0.0.1" | "localhost")) {
            anyhow::bail!("loadout_backend_invalid");
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            base: url,
            token,
            client,
        })
    }

    fn url(&self, path: &str) -> anyhow::Result<reqwest::Url> {
        self.base.join(path).map_err(Into::into)
    }

    async fn decode<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> anyhow::Result<T> {
        if !response.status().is_success() {
            anyhow::bail!("labby_agent_failed:{}", response.status().as_u16());
        }
        let bytes = response.bytes().await?;
        if bytes.len() > 1024 * 1024 {
            anyhow::bail!("labby_agent_payload_too_large");
        }
        serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("labby_agent_contract_invalid"))
    }

    pub async fn create_context(
        &self,
        delegation: &str,
        loadout: &str,
        revision: u64,
        expires: i64,
    ) -> anyhow::Result<LabbyContextReceipt> {
        let response = self.client.post(self.url("/v1/palette/agent/contexts")?).bearer_auth(&self.token).json(&serde_json::json!({"delegationToken":delegation,"loadoutId":loadout,"loadoutRevision":revision,"expiresAtUnixMs":expires})).send().await?;
        let receipt: LabbyContextReceipt = self.decode(response).await?;
        if receipt.loadout_id != loadout
            || receipt.loadout_revision != revision
            || receipt.expires_at_unix_ms != expires
            || receipt.execution_context_id.is_empty()
            || receipt.actor.is_empty()
            || receipt.service.is_empty()
        {
            anyhow::bail!("labby_context_binding_mismatch");
        }
        Ok(receipt)
    }

    pub async fn request_approval(
        &self,
        context: &str,
        proposal: &AgentToolProposal,
    ) -> anyhow::Result<LabbyApprovalChallenge> {
        let response = self.client.post(self.url("/v1/palette/agent/approvals")?).bearer_auth(&self.token).json(&serde_json::json!({"executionContextId":context,"id":proposal.tool_id,"params":proposal.arguments,"expectedContractHash":proposal.contract_hash})).send().await?;
        self.decode(response).await
    }

    pub async fn execute(
        &self,
        context: &str,
        key: &str,
        proposal: &AgentToolProposal,
        approval: Option<&str>,
        deadline: i64,
    ) -> anyhow::Result<LabbyExecutionReceipt> {
        let response = self.client.post(self.url("/v1/palette/agent/executions")?).bearer_auth(&self.token).json(&serde_json::json!({"executionContextId":context,"idempotencyKey":key,"id":proposal.tool_id,"params":proposal.arguments,"expectedContractHash":proposal.contract_hash,"approvalToken":approval,"deadlineAtUnixMs":deadline})).send().await?;
        self.decode(response).await
    }

    pub async fn status(&self, id: &str) -> anyhow::Result<LabbyExecutionReceipt> {
        let id = percent_encoding::utf8_percent_encode(id, percent_encoding::NON_ALPHANUMERIC);
        let response = self
            .client
            .get(self.url(&format!("/v1/palette/agent/executions/{id}"))?)
            .bearer_auth(&self.token)
            .send()
            .await?;
        self.decode(response).await
    }

    pub async fn cancel(&self, id: &str) -> anyhow::Result<LabbyExecutionReceipt> {
        let id = percent_encoding::utf8_percent_encode(id, percent_encoding::NON_ALPHANUMERIC);
        let response = self
            .client
            .post(self.url(&format!("/v1/palette/agent/executions/{id}/cancel"))?)
            .bearer_auth(&self.token)
            .send()
            .await?;
        self.decode(response).await
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LabbyApprovalChallenge {
    pub approval_token: String,
    pub approval_id: String,
    pub expires_at_unix_ms: i64,
}
