//! `ServiceJob` — the rich, transport-neutral job view returned by the service
//! job runtime to CLI/MCP/HTTP callers.
//!
//! Lives here in `axon-api` (not `services`) so `axon-jobs` can construct it
//! directly (breaking the historical `jobs` ↔ `services` cycle). The
//! `From<jobs::*Job>` conversions live in `axon-jobs`, where the source job
//! types are local.

use crate::job_status::JobStatus;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ServiceJob {
    pub id: uuid::Uuid,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error_text: Option<String>,
    pub url: Option<String>,
    pub source_type: Option<String>,
    pub target: Option<String>,
    pub urls_json: Option<serde_json::Value>,
    pub progress_json: Option<serde_json::Value>,
    pub result_json: Option<serde_json::Value>,
    pub config_json: Option<serde_json::Value>,
    pub attempt_count: i64,
    pub active_attempt_id: Option<String>,
    pub last_reclaimed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_reclaimed_reason: Option<String>,
}

impl ServiceJob {
    pub fn status_enum(&self) -> JobStatus {
        JobStatus::from_str(&self.status)
    }

    pub fn wire_json_compat(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}));
        let Some(obj) = value.as_object_mut() else {
            return value;
        };
        let active = self.status_enum().is_active();
        let metrics = if active {
            usable_progress_json(self.progress_json.as_ref()).or(self.result_json.as_ref())
        } else {
            self.result_json.as_ref()
        };
        if let Some(metrics) = metrics {
            obj.insert("metrics".to_string(), metrics.clone());
            if active {
                obj.insert("result_json".to_string(), metrics.clone());
            }
        }
        value
    }
}

fn usable_progress_json(value: Option<&serde_json::Value>) -> Option<&serde_json::Value> {
    value.filter(|value| {
        !(value.get("degraded").and_then(serde_json::Value::as_bool) == Some(true)
            && value.get("field").and_then(serde_json::Value::as_str) == Some("progress_json"))
    })
}
