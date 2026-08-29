//! Single-use, action-bound human approval prompts.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalRisk {
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalPrompt {
    pub id: u64,
    pub action: String,
    pub origin: String,
    pub target: String,
    pub risk: ApprovalRisk,
    pub summary: String,
    pub effect: String,
    pub digest: String,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approve,
    Deny,
}

pub struct ApprovalRequest<'a> {
    pub action: String,
    pub origin: String,
    pub target: String,
    pub risk: ApprovalRisk,
    pub summary: String,
    pub effect: String,
    pub params: &'a Value,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
struct PendingApproval {
    prompt: ApprovalPrompt,
    expires_at: Instant,
}

#[derive(Debug, Clone, Default)]
pub struct ApprovalService {
    next_id: Arc<AtomicU64>,
    pending: Arc<Mutex<HashMap<u64, PendingApproval>>>,
}

impl ApprovalService {
    pub fn create(&self, request: ApprovalRequest<'_>) -> Result<ApprovalPrompt, String> {
        let ApprovalRequest {
            action,
            origin,
            target,
            risk,
            summary,
            effect,
            params,
            timeout,
        } = request;
        if timeout.is_zero() {
            return Err("approval timeout must be positive".to_string());
        }
        let id = self
            .next_id
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        let digest = approval_digest(&action, &origin, &target, params)?;
        let prompt = ApprovalPrompt {
            id,
            action,
            origin,
            target,
            risk,
            summary,
            effect,
            digest,
            timeout_seconds: timeout.as_secs().max(1),
        };
        self.pending
            .lock()
            .map_err(|_| "approval store lock poisoned".to_string())?
            .insert(
                id,
                PendingApproval {
                    prompt: prompt.clone(),
                    expires_at: Instant::now() + timeout,
                },
            );
        Ok(prompt)
    }

    pub fn decide(
        &self,
        id: u64,
        expected_digest: &str,
        decision: ApprovalDecision,
    ) -> Result<ApprovalDecision, String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "approval store lock poisoned".to_string())?;
        let approval = pending
            .get(&id)
            .ok_or_else(|| "approval is unknown or already consumed".to_string())?;
        if approval.expires_at <= Instant::now() {
            pending.remove(&id);
            return Err("approval expired".to_string());
        }
        if approval.prompt.digest != expected_digest {
            return Err("approval digest mismatch".to_string());
        }
        pending.remove(&id);
        Ok(decision)
    }

    pub fn list(&self) -> Result<Vec<ApprovalPrompt>, String> {
        let now = Instant::now();
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "approval store lock poisoned".to_string())?;
        pending.retain(|_, value| value.expires_at > now);
        let mut prompts: Vec<_> = pending.values().map(|value| value.prompt.clone()).collect();
        prompts.sort_by_key(|prompt| prompt.id);
        Ok(prompts)
    }
}

fn approval_digest(
    action: &str,
    origin: &str,
    target: &str,
    params: &Value,
) -> Result<String, String> {
    let canonical = serde_json::to_vec(params)
        .map_err(|error| format!("failed to encode approval parameters: {error}"))?;
    let mut digest = Sha256::new();
    for value in [
        action.as_bytes(),
        origin.as_bytes(),
        target.as_bytes(),
        &canonical,
    ] {
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
#[path = "approval_tests.rs"]
mod tests;
