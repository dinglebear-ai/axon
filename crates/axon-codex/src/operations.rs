//! Durable SQLite ledger for Codex control mutations and approvals.

use rand::RngCore;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::{Arc, Mutex};
use utoipa::ToSchema;

#[path = "operations_migrations.rs"]
mod migrations;
use migrations::migrate_columns;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ControlOperation {
    pub id: i64,
    pub actor: String,
    pub scope: String,
    pub method: String,
    pub request_digest: String,
    /// Secret-free request retained so recovery can prove the intended effect.
    pub redacted_request: serde_json::Value,
    pub expected_revision: Option<String>,
    pub phase: OperationPhase,
    pub approver: Option<String>,
    pub post_state_revision: Option<String>,
    pub recovery_state: Option<String>,
    pub execution_attempt_id: Option<String>,
    pub response_evidence: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecoveryOperation {
    pub operation: ControlOperation,
    pub target_home_identity: String,
    pub runtime_boot_id: u64,
    pub policy_version: String,
}

#[derive(Debug, Clone)]
pub struct OperationStore {
    connection: Arc<Mutex<Connection>>,
}

impl OperationStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        let connection = Connection::open(path).map_err(db_error)?;
        connection.execute_batch(SCHEMA).map_err(db_error)?;
        migrate_columns(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn open_memory() -> Result<Self, String> {
        let connection = Connection::open_in_memory().map_err(db_error)?;
        connection.execute_batch(SCHEMA).map_err(db_error)?;
        migrate_columns(&connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn create(&self, intent: &OperationIntent) -> Result<ControlOperation, String> {
        let digest = request_digest(&intent.redacted_request)?;
        let mut nonce = [0_u8; 32];
        rand::thread_rng().fill_bytes(&mut nonce);
        let nonce = hex(&nonce);
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "operation store lock poisoned".to_string())?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        if let Some(existing) =
            find_idempotent(&transaction, &intent.actor, &intent.idempotency_key)?
        {
            if existing.scope != intent.scope
                || existing.method != intent.method
                || existing.request_digest != digest
            {
                return Err("idempotency key already exists with different parameters".to_string());
            }
            let guard = get_operation_guard(&transaction, existing.id)?
                .ok_or_else(|| "idempotent operation guard missing".to_string())?;
            if guard.target_home_identity != intent.target_home_identity
                || guard.policy_version != intent.policy_version
            {
                return Err(
                    "idempotency key belongs to a different control target or policy".to_string(),
                );
            }
            return Ok(existing);
        }
        transaction.execute(
            "INSERT INTO codex_control_operations(actor,scope,method,target_home_identity,runtime_boot_id,policy_version,expected_revision,idempotency_key,request_digest,redacted_request,nonce,phase,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'pending',unixepoch(),unixepoch())",
            params![intent.actor,intent.scope,intent.method,intent.target_home_identity,intent.runtime_boot_id,intent.policy_version,intent.expected_revision,intent.idempotency_key,digest,serde_json::to_string(&intent.redacted_request).map_err(|error| error.to_string())?,nonce],
        ).map_err(db_error)?;
        let id = transaction.last_insert_rowid();
        transaction.commit().map_err(db_error)?;
        get_operation(&connection, id)?.ok_or_else(|| "created operation missing".to_string())
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
        let changed = connection.execute("UPDATE codex_control_operations SET phase='approved',approver=?2,approval_digest=?3,approved_at=unixepoch(),expires_at=unixepoch()+300,updated_at=unixepoch() WHERE id=?1 AND phase='pending'", params![id,approver,capability_digest]).map_err(db_error)?;
        if changed != 1 {
            return Err("operation is not pending approval".to_string());
        }
        Ok(capability)
    }

    pub fn cancel(&self, id: i64) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "operation store lock poisoned".to_string())?;
        let changed = connection.execute(
            "UPDATE codex_control_operations SET phase='denied',approval_digest=NULL,recovery_state='operator_cancelled',updated_at=unixepoch() WHERE id=?1 AND phase IN ('pending','approved')",
            [id],
        ).map_err(db_error)?;
        (changed == 1)
            .then_some(())
            .ok_or_else(|| "operation cannot be cancelled".to_string())
    }

    pub fn begin_execution(
        &self,
        id: i64,
        capability: &str,
        method: &str,
        request: &serde_json::Value,
        current_revision: Option<&str>,
        home_identity: &str,
        runtime_boot_id: u64,
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
        let supplied_request_digest = request_digest(request)?;
        if row.phase != OperationPhase::Approved || row.approval_digest.as_deref() != Some(&digest)
        {
            return Err("approval capability is invalid or already consumed".to_string());
        }
        if row
            .expires_at
            .is_none_or(|expires| expires <= unix_timestamp())
        {
            transaction.execute("UPDATE codex_control_operations SET phase='expired',approval_digest=NULL,updated_at=unixepoch() WHERE id=?1", [id]).map_err(db_error)?;
            transaction.commit().map_err(db_error)?;
            return Err("approval capability expired; reapproval required".to_string());
        }
        if row.method != method || row.request_digest != supplied_request_digest {
            return Err("operation method or parameters changed; reapproval required".to_string());
        }
        if row.target_home_identity != home_identity
            || row.runtime_boot_id != runtime_boot_id
            || row.policy_version != policy_version
        {
            return Err(
                "operation target, runtime, or policy changed; reapproval required".to_string(),
            );
        }
        if row.expected_revision.as_deref() != current_revision {
            return Err("operation revision is stale; refresh and reapprove".to_string());
        }
        let mut attempt = [0_u8; 32];
        rand::thread_rng().fill_bytes(&mut attempt);
        let attempt = hex(&attempt);
        let changed = transaction.execute("UPDATE codex_control_operations SET phase='executing',approval_digest=NULL,execution_attempt_id=?2,response_evidence=NULL,side_effect_started_at=unixepoch(),updated_at=unixepoch() WHERE id=?1 AND phase='approved'", params![id, attempt]).map_err(db_error)?;
        if changed != 1 {
            return Err("approval capability was consumed concurrently".to_string());
        }
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

    pub fn record_response_evidence(
        &self,
        id: i64,
        response: &serde_json::Value,
    ) -> Result<String, String> {
        let evidence = request_digest(response)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "operation store lock poisoned".to_string())?;
        let changed = connection.execute(
            "UPDATE codex_control_operations SET response_evidence=?2,updated_at=unixepoch() WHERE id=?1 AND phase='executing'",
            params![id, evidence],
        ).map_err(db_error)?;
        (changed == 1)
            .then_some(evidence)
            .ok_or_else(|| "operation is not executing".to_string())
    }

    /// Resolve an unprovable one-shot without replaying it. `applied` records the
    /// operator's explicit assessment; it never invokes the external method.
    pub fn resolve_without_replay(&self, id: i64, applied: bool, note: &str) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "operation store lock poisoned".to_string())?;
        let phase = if applied { "reconciled" } else { "failed" };
        let state = format!("operator_disposition_without_replay:{note}");
        let changed = connection.execute(
            "UPDATE codex_control_operations SET phase=?2,recovery_state=?3,updated_at=unixepoch() WHERE id=?1 AND phase IN ('executing','ambiguous','recovery_required','rollback_required')",
            params![id, phase, state],
        ).map_err(db_error)?;
        (changed == 1)
            .then_some(())
            .ok_or_else(|| "operation is not awaiting recovery".to_string())
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

    pub fn get_for_recovery(&self, id: i64) -> Result<Option<RecoveryOperation>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "operation store lock poisoned".to_string())?;
        let operation = get_operation(&connection, id)?;
        let Some(operation) = operation else {
            return Ok(None);
        };
        let guard = get_operation_guard(&connection, id)?
            .ok_or_else(|| "operation recovery guard missing".to_string())?;
        Ok(Some(RecoveryOperation {
            operation,
            target_home_identity: guard.target_home_identity,
            runtime_boot_id: guard.runtime_boot_id,
            policy_version: guard.policy_version,
        }))
    }

    pub fn unfinished(&self, limit: usize) -> Result<Vec<ControlOperation>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "operation store lock poisoned".to_string())?;
        connection.execute(
            "UPDATE codex_control_operations SET phase='expired',approval_digest=NULL,updated_at=unixepoch() WHERE phase='approved' AND expires_at <= unixepoch()",
            [],
        ).map_err(db_error)?;
        let mut statement = connection.prepare("SELECT id,actor,scope,method,request_digest,redacted_request,expected_revision,phase,approver,post_state_revision,recovery_state FROM codex_control_operations WHERE phase NOT IN ('reconciled','failed','denied','expired') ORDER BY updated_at DESC LIMIT ?1").map_err(db_error)?;
        statement
            .query_map([limit.min(100) as i64], row_to_operation)
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)
    }

    pub fn resolve_recovery(&self, id: i64, revision: &str) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "operation store lock poisoned".to_string())?;
        let changed = connection.execute("UPDATE codex_control_operations SET phase='reconciled',post_state_revision=?2,recovery_state='operator_reconciled',updated_at=unixepoch() WHERE id=?1 AND phase IN ('executing','ambiguous','recovery_required','rollback_required')", params![id,revision]).map_err(db_error)?;
        (changed == 1)
            .then_some(())
            .ok_or_else(|| "operation is not awaiting recovery".to_string())
    }

    /// Keep an operation safely unresolved when readback cannot prove its effect.
    pub fn retain_recovery(&self, id: i64, reason: &str) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "operation store lock poisoned".to_string())?;
        let changed = connection.execute(
            "UPDATE codex_control_operations SET phase='recovery_required',recovery_state=?2,updated_at=unixepoch() WHERE id=?1 AND phase IN ('executing','ambiguous','recovery_required','rollback_required')",
            params![id, reason],
        ).map_err(db_error)?;
        (changed == 1)
            .then_some(())
            .ok_or_else(|| "operation is not awaiting recovery".to_string())
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
    runtime_boot_id: u64,
    policy_version: String,
    expected_revision: Option<String>,
    method: String,
    request_digest: String,
    expires_at: Option<i64>,
}
fn get_operation_guard(connection: &Connection, id: i64) -> Result<Option<GuardRow>, String> {
    connection.query_row("SELECT phase,approval_digest,target_home_identity,runtime_boot_id,policy_version,expected_revision,method,request_digest,expires_at FROM codex_control_operations WHERE id=?1", [id], |row| Ok(GuardRow { phase: OperationPhase::parse(&row.get::<_,String>(0)?).map_err(|_| rusqlite::Error::InvalidQuery)?, approval_digest: row.get(1)?, target_home_identity: row.get(2)?, runtime_boot_id: row.get(3)?, policy_version: row.get(4)?, expected_revision: row.get(5)?, method: row.get(6)?, request_digest: row.get(7)?, expires_at: row.get(8)? })).optional().map_err(db_error)
}
fn get_operation(connection: &Connection, id: i64) -> Result<Option<ControlOperation>, String> {
    connection
        .query_row(
            &format!("{OPERATION_SELECT} WHERE id=?1"),
            [id],
            row_to_operation,
        )
        .optional()
        .map_err(db_error)
}
fn find_idempotent(
    connection: &Connection,
    actor: &str,
    key: &str,
) -> Result<Option<ControlOperation>, String> {
    connection
        .query_row(
            &format!("{OPERATION_SELECT} WHERE actor=?1 AND idempotency_key=?2"),
            params![actor, key],
            row_to_operation,
        )
        .optional()
        .map_err(db_error)
}
fn row_to_operation(row: &rusqlite::Row<'_>) -> rusqlite::Result<ControlOperation> {
    Ok(ControlOperation {
        id: row.get(0)?,
        actor: row.get(1)?,
        scope: row.get(2)?,
        method: row.get(3)?,
        request_digest: row.get(4)?,
        redacted_request: serde_json::from_str(&row.get::<_, String>(5)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        expected_revision: row.get(6)?,
        phase: OperationPhase::parse(&row.get::<_, String>(7)?)
            .map_err(|_| rusqlite::Error::InvalidQuery)?,
        approver: row.get(8)?,
        post_state_revision: row.get(9)?,
        recovery_state: row.get(10)?,
        execution_attempt_id: row.get(11)?,
        response_evidence: row.get(12)?,
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

const OPERATION_SELECT: &str = "SELECT id,actor,scope,method,request_digest,redacted_request,expected_revision,phase,approver,post_state_revision,recovery_state,execution_attempt_id,response_evidence FROM codex_control_operations";
const SCHEMA: &str = "PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS codex_control_operations(id INTEGER PRIMARY KEY,actor TEXT NOT NULL,scope TEXT NOT NULL,method TEXT NOT NULL,target_home_identity TEXT NOT NULL,runtime_boot_id INTEGER NOT NULL,policy_version TEXT NOT NULL,expected_revision TEXT,idempotency_key TEXT NOT NULL,request_digest TEXT NOT NULL,redacted_request TEXT NOT NULL,nonce TEXT NOT NULL,phase TEXT NOT NULL,approver TEXT,approval_digest TEXT,approved_at INTEGER,expires_at INTEGER,side_effect_started_at INTEGER,post_state_revision TEXT,recovery_state TEXT,execution_attempt_id TEXT,response_evidence TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,UNIQUE(actor,idempotency_key)); CREATE INDEX IF NOT EXISTS idx_codex_control_phase_updated ON codex_control_operations(phase,updated_at);";

fn unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |value| value.as_secs() as i64)
}

#[cfg(test)]
#[path = "operations_tests.rs"]
mod tests;
