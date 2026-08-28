//! Durable SQLite ledger for Codex control mutations and approvals.

use rand::RngCore;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationPhase {
    Pending,
    Approved,
    Denied,
    Expired,
    Executing,
    Reconciled,
    Failed,
    Ambiguous,
    RollbackRequired,
    RecoveryRequired,
}

impl OperationPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Expired => "expired",
            Self::Executing => "executing",
            Self::Reconciled => "reconciled",
            Self::Failed => "failed",
            Self::Ambiguous => "ambiguous",
            Self::RollbackRequired => "rollback_required",
            Self::RecoveryRequired => "recovery_required",
        }
    }
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "denied" => Ok(Self::Denied),
            "expired" => Ok(Self::Expired),
            "executing" => Ok(Self::Executing),
            "reconciled" => Ok(Self::Reconciled),
            "failed" => Ok(Self::Failed),
            "ambiguous" => Ok(Self::Ambiguous),
            "rollback_required" => Ok(Self::RollbackRequired),
            "recovery_required" => Ok(Self::RecoveryRequired),
            other => Err(format!("unknown Codex operation phase: {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationIntent {
    pub actor: String,
    pub scope: String,
    pub method: String,
    pub target_home_identity: String,
    pub runtime_boot_id: u64,
    pub policy_version: String,
    pub expected_revision: Option<String>,
    pub idempotency_key: String,
    pub redacted_request: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlOperation {
    pub id: i64,
    pub actor: String,
    pub scope: String,
    pub method: String,
    pub request_digest: String,
    pub phase: OperationPhase,
    pub approver: Option<String>,
    pub post_state_revision: Option<String>,
    pub recovery_state: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OperationStore {
    connection: Arc<Mutex<Connection>>,
}

impl OperationStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        let connection = Connection::open(path).map_err(db_error)?;
        connection.execute_batch(SCHEMA).map_err(db_error)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn open_memory() -> Result<Self, String> {
        let connection = Connection::open_in_memory().map_err(db_error)?;
        connection.execute_batch(SCHEMA).map_err(db_error)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn create(&self, intent: &OperationIntent) -> Result<ControlOperation, String> {
        let digest = request_digest(&intent.redacted_request)?;
        let mut nonce = [0_u8; 32];
        rand::thread_rng().fill_bytes(&mut nonce);
        let nonce = hex(&nonce);
        let connection = self
            .connection
            .lock()
            .map_err(|_| "operation store lock poisoned".to_string())?;
        if let Some(existing) =
            find_idempotent(&connection, &intent.actor, &intent.idempotency_key)?
        {
            if existing.request_digest != digest {
                return Err("idempotency key already exists with different parameters".to_string());
            }
            return Ok(existing);
        }
        connection.execute(
            "INSERT INTO codex_control_operations(actor,scope,method,target_home_identity,runtime_boot_id,policy_version,expected_revision,idempotency_key,request_digest,nonce,phase,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'pending',unixepoch(),unixepoch())",
            params![intent.actor,intent.scope,intent.method,intent.target_home_identity,intent.runtime_boot_id,intent.policy_version,intent.expected_revision,intent.idempotency_key,digest,nonce],
        ).map_err(db_error)?;
        get_operation(&connection, connection.last_insert_rowid())?
            .ok_or_else(|| "created operation missing".to_string())
    }

    pub fn approve(&self, id: i64, approver: &str) -> Result<String, String> {
        let mut capability = [0_u8; 32];
        rand::thread_rng().fill_bytes(&mut capability);
        let capability = hex(&capability);
        let capability_digest = request_digest(&serde_json::Value::String(capability.clone()))?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "operation store lock poisoned".to_string())?;
        let changed = connection.execute("UPDATE codex_control_operations SET phase='approved',approver=?2,approval_digest=?3,updated_at=unixepoch() WHERE id=?1 AND phase='pending'", params![id,approver,capability_digest]).map_err(db_error)?;
        if changed != 1 {
            return Err("operation is not pending approval".to_string());
        }
        Ok(capability)
    }

    pub fn begin_execution(
        &self,
        id: i64,
        capability: &str,
        current_revision: Option<&str>,
        home_identity: &str,
        policy_version: &str,
    ) -> Result<ControlOperation, String> {
        let digest = request_digest(&serde_json::Value::String(capability.to_string()))?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "operation store lock poisoned".to_string())?;
        let transaction = connection.transaction().map_err(db_error)?;
        let row = get_operation_guard(&transaction, id)?
            .ok_or_else(|| "operation not found".to_string())?;
        if row.phase != OperationPhase::Approved || row.approval_digest.as_deref() != Some(&digest)
        {
            return Err("approval capability is invalid or already consumed".to_string());
        }
        if row.target_home_identity != home_identity || row.policy_version != policy_version {
            return Err("operation target or policy changed; reapproval required".to_string());
        }
        if row.expected_revision.as_deref() != current_revision {
            return Err("operation revision is stale; refresh and reapprove".to_string());
        }
        transaction.execute("UPDATE codex_control_operations SET phase='executing',approval_digest=NULL,side_effect_started_at=unixepoch(),updated_at=unixepoch() WHERE id=?1 AND phase='approved'", [id]).map_err(db_error)?;
        transaction.commit().map_err(db_error)?;
        get_operation(&connection, id)?.ok_or_else(|| "operation disappeared".to_string())
    }

    pub fn reconcile(&self, id: i64, revision: &str) -> Result<(), String> {
        self.transition(
            id,
            OperationPhase::Executing,
            OperationPhase::Reconciled,
            Some(revision),
            None,
        )
    }

    pub fn fail_ambiguous(&self, id: i64, reason: &str) -> Result<(), String> {
        self.transition(
            id,
            OperationPhase::Executing,
            OperationPhase::Ambiguous,
            None,
            Some(reason),
        )
    }

    pub fn recover_interrupted(&self) -> Result<usize, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "operation store lock poisoned".to_string())?;
        connection.execute("UPDATE codex_control_operations SET phase='recovery_required',recovery_state='interrupted_while_executing',updated_at=unixepoch() WHERE phase='executing'", []).map_err(db_error)
    }

    pub fn get(&self, id: i64) -> Result<Option<ControlOperation>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "operation store lock poisoned".to_string())?;
        get_operation(&connection, id)
    }

    fn transition(
        &self,
        id: i64,
        from: OperationPhase,
        to: OperationPhase,
        revision: Option<&str>,
        recovery: Option<&str>,
    ) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "operation store lock poisoned".to_string())?;
        let changed = connection.execute("UPDATE codex_control_operations SET phase=?3,post_state_revision=?4,recovery_state=?5,updated_at=unixepoch() WHERE id=?1 AND phase=?2", params![id,from.as_str(),to.as_str(),revision,recovery]).map_err(db_error)?;
        if changed == 1 {
            Ok(())
        } else {
            Err(format!(
                "invalid operation transition from {}",
                from.as_str()
            ))
        }
    }
}

struct GuardRow {
    phase: OperationPhase,
    approval_digest: Option<String>,
    target_home_identity: String,
    policy_version: String,
    expected_revision: Option<String>,
}
fn get_operation_guard(connection: &Connection, id: i64) -> Result<Option<GuardRow>, String> {
    connection.query_row("SELECT phase,approval_digest,target_home_identity,policy_version,expected_revision FROM codex_control_operations WHERE id=?1", [id], |row| Ok(GuardRow { phase: OperationPhase::parse(&row.get::<_,String>(0)?).map_err(|_| rusqlite::Error::InvalidQuery)?, approval_digest: row.get(1)?, target_home_identity: row.get(2)?, policy_version: row.get(3)?, expected_revision: row.get(4)? })).optional().map_err(db_error)
}
fn get_operation(connection: &Connection, id: i64) -> Result<Option<ControlOperation>, String> {
    connection.query_row("SELECT id,actor,scope,method,request_digest,phase,approver,post_state_revision,recovery_state FROM codex_control_operations WHERE id=?1", [id], row_to_operation).optional().map_err(db_error)
}
fn find_idempotent(
    connection: &Connection,
    actor: &str,
    key: &str,
) -> Result<Option<ControlOperation>, String> {
    connection.query_row("SELECT id,actor,scope,method,request_digest,phase,approver,post_state_revision,recovery_state FROM codex_control_operations WHERE actor=?1 AND idempotency_key=?2", params![actor,key], row_to_operation).optional().map_err(db_error)
}
fn row_to_operation(row: &rusqlite::Row<'_>) -> rusqlite::Result<ControlOperation> {
    Ok(ControlOperation {
        id: row.get(0)?,
        actor: row.get(1)?,
        scope: row.get(2)?,
        method: row.get(3)?,
        request_digest: row.get(4)?,
        phase: OperationPhase::parse(&row.get::<_, String>(5)?)
            .map_err(|_| rusqlite::Error::InvalidQuery)?,
        approver: row.get(6)?,
        post_state_revision: row.get(7)?,
        recovery_state: row.get(8)?,
    })
}
fn request_digest(value: &serde_json::Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn db_error(error: rusqlite::Error) -> String {
    format!("Codex operation database error: {error}")
}

const SCHEMA: &str = "PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS codex_control_operations(id INTEGER PRIMARY KEY,actor TEXT NOT NULL,scope TEXT NOT NULL,method TEXT NOT NULL,target_home_identity TEXT NOT NULL,runtime_boot_id INTEGER NOT NULL,policy_version TEXT NOT NULL,expected_revision TEXT,idempotency_key TEXT NOT NULL,request_digest TEXT NOT NULL,nonce TEXT NOT NULL,phase TEXT NOT NULL,approver TEXT,approval_digest TEXT,side_effect_started_at INTEGER,post_state_revision TEXT,recovery_state TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,UNIQUE(actor,idempotency_key)); CREATE INDEX IF NOT EXISTS idx_codex_control_phase_updated ON codex_control_operations(phase,updated_at);";

#[cfg(test)]
#[path = "operations_tests.rs"]
mod tests;
