use super::{LabbyExecutionReceipt, client::LabbyContextReceipt};
use axon_api::agent::{
    AgentCorrelation, AgentEvent, AgentToolProposal, AgentTurnResult, AgentTurnStatus,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Mutex};

#[derive(Debug, Clone)]
pub struct StoredTurn {
    pub id: String,
    pub loadout_id: String,
    pub loadout_revision: u64,
    pub prompt: String,
    pub prompt_hash: String,
    pub execution_context_id: String,
    pub actor: String,
    pub service: String,
    pub status: AgentTurnStatus,
    pub deadline_at_ms: i64,
    pub tool_call_count: u32,
    pub pending_proposal: Option<AgentToolProposal>,
    pub tool_results: Vec<Value>,
    pub active_request_id: Option<String>,
    pub owner: String,
    pub profile_id: String,
    pub max_tool_calls: u32,
    pub model: String,
    pub version: u64,
    pub cancel_requested: bool,
}

impl StoredTurn {
    pub fn verify_resume(&self, owner: &str) -> anyhow::Result<()> {
        if self.owner != owner {
            anyhow::bail!("agent_turn_not_found");
        }
        if self.status.is_terminal() {
            anyhow::bail!("agent_turn_terminal");
        }
        if self.cancel_requested {
            anyhow::bail!("agent_turn_cancelled");
        }
        Ok(())
    }

    pub fn verify_create_replay(
        &self,
        owner: &str,
        profile: &str,
        loadout: &str,
        revision: u64,
        prompt: &str,
    ) -> anyhow::Result<()> {
        if self.owner != owner || self.profile_id != profile {
            anyhow::bail!("agent_turn_not_found");
        }
        if self.loadout_id != loadout
            || self.loadout_revision != revision
            || self.prompt_hash != digest(prompt)
        {
            anyhow::bail!("agent_turn_idempotency_mismatch");
        }
        Ok(())
    }
}

pub struct AgentTurnStore {
    conn: Mutex<Connection>,
}

impl AgentTurnStore {
    pub fn open(path: PathBuf) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate(&conn)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.maintain(now_ms(), 30 * 24 * 60 * 60 * 1000)?;
        Ok(store)
    }
    #[cfg(test)]
    pub fn memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn create(
        &self,
        id: &str,
        loadout: &str,
        revision: u64,
        prompt: &str,
        deadline: i64,
        owner: &str,
        profile: &str,
        max_tool_calls: u32,
        model: &str,
        context: &LabbyContextReceipt,
    ) -> anyhow::Result<StoredTurn> {
        self.conn.lock().unwrap().execute("INSERT INTO agent_turns(id,loadout_id,loadout_revision,prompt,prompt_hash,execution_context_id,actor,service,status,deadline_at_ms,tool_call_count,tool_results_json,sequence,owner,profile_id,max_tool_calls,model,version,cancel_requested) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'pending',?9,0,'[]',0,?10,?11,?12,?13,0,0)",params![id,loadout,revision as i64,prompt,digest(prompt),context.execution_context_id,context.actor,context.service,deadline,owner,profile,max_tool_calls,model])?;
        self.load(id)?
            .ok_or_else(|| anyhow::anyhow!("agent_turn_create_failed"))
    }
    pub fn load(&self, id: &str) -> anyhow::Result<Option<StoredTurn>> {
        self.conn.lock().unwrap().query_row("SELECT id,loadout_id,loadout_revision,prompt,prompt_hash,execution_context_id,actor,service,status,deadline_at_ms,tool_call_count,pending_proposal_json,tool_results_json,active_request_id,owner,profile_id,max_tool_calls,model,version,cancel_requested FROM agent_turns WHERE id=?1",[id],|r|{
            let status:String=r.get(8)?; let proposal:Option<String>=r.get(11)?; let results:String=r.get(12)?;
            Ok(StoredTurn{id:r.get(0)?,loadout_id:r.get(1)?,loadout_revision:r.get::<_,i64>(2)? as u64,prompt:r.get(3)?,prompt_hash:r.get(4)?,execution_context_id:r.get(5)?,actor:r.get(6)?,service:r.get(7)?,status:parse_status(&status),deadline_at_ms:r.get(9)?,tool_call_count:r.get::<_,i64>(10)? as u32,pending_proposal:proposal.and_then(|v|serde_json::from_str(&v).ok()),tool_results:serde_json::from_str(&results).unwrap_or_default(),active_request_id:r.get(13)?,owner:r.get(14)?,profile_id:r.get(15)?,max_tool_calls:r.get::<_,i64>(16)? as u32,model:r.get(17)?,version:r.get::<_,i64>(18)? as u64,cancel_requested:r.get::<_,i64>(19)? != 0})
        }).optional().map_err(Into::into)
    }
    pub fn load_owned(&self, id: &str, owner: &str) -> anyhow::Result<StoredTurn> {
        let turn = self
            .load(id)?
            .ok_or_else(|| anyhow::anyhow!("agent_turn_not_found"))?;
        if turn.owner != owner {
            anyhow::bail!("agent_turn_not_found");
        }
        Ok(turn)
    }
    pub fn acquire_lease(
        &self,
        id: &str,
        owner: &str,
        expected: u64,
        now: i64,
    ) -> anyhow::Result<u64> {
        let changed=self.conn.lock().unwrap().execute("UPDATE agent_turns SET lease_until_ms=?1,version=version+1 WHERE id=?2 AND owner=?3 AND version=?4 AND cancel_requested=0 AND status NOT IN ('succeeded','failed','cancelled','timed_out') AND lease_until_ms<=?5",params![now+30_000,id,owner,expected as i64,now])?;
        if changed != 1 {
            anyhow::bail!("agent_turn_resume_conflict");
        }
        Ok(expected + 1)
    }
    pub fn release_lease(&self, id: &str, version: u64) -> anyhow::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE agent_turns SET lease_until_ms=0 WHERE id=?1 AND version=?2",
            params![id, version as i64],
        )?;
        Ok(())
    }
    pub fn request_cancel(&self, id: &str, owner: &str) -> anyhow::Result<StoredTurn> {
        let changed=self.conn.lock().unwrap().execute("UPDATE agent_turns SET cancel_requested=1,status=CASE WHEN active_request_id IS NULL THEN 'cancelled' ELSE 'cancel_unconfirmed' END,version=version+1 WHERE id=?1 AND owner=?2 AND status NOT IN ('succeeded','failed','cancelled','timed_out')",params![id,owner])?;
        if changed == 0 {
            return self.load_owned(id, owner);
        }
        self.load_owned(id, owner)
    }
    pub fn confirm_cancel(&self, id: &str, owner: &str) -> anyhow::Result<()> {
        self.conn.lock().unwrap().execute("UPDATE agent_turns SET status='cancelled',active_request_id=NULL WHERE id=?1 AND owner=?2 AND cancel_requested=1",params![id,owner])?;
        Ok(())
    }
    pub fn maintain(&self, now_ms: i64, retention_ms: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE agent_turns SET status='interrupted',lease_until_ms=0,version=version+1 WHERE lease_until_ms>0 AND lease_until_ms<=?1 AND cancel_requested=0",[now_ms])?;
        conn.execute("DELETE FROM agent_turns WHERE status IN ('succeeded','failed','cancelled','timed_out') AND deadline_at_ms<?1",[now_ms.saturating_sub(retention_ms)])?;
        Ok(())
    }
    pub fn transition(
        &self,
        id: &str,
        status: AgentTurnStatus,
        value: Option<&str>,
    ) -> anyhow::Result<()> {
        let changed=self.conn.lock().unwrap().execute("UPDATE agent_turns SET status=?1,answer=CASE WHEN ?2 IS NULL THEN answer ELSE ?2 END,error_kind=CASE WHEN ?3 THEN ?2 ELSE error_kind END WHERE id=?4 AND cancel_requested=0",params![status_str(&status),value,!matches!(status,AgentTurnStatus::Succeeded),id])?;
        if changed == 0 {
            return Ok(());
        }
        self.append_event(
            id,
            AgentEvent::State {
                sequence: 0,
                status,
            },
        )
    }
    pub fn set_proposal(&self, id: &str, p: &AgentToolProposal) -> anyhow::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE agent_turns SET pending_proposal_json=?1 WHERE id=?2",
            params![serde_json::to_string(p)?, id],
        )?;
        self.append_event(
            id,
            AgentEvent::ModelProposal {
                sequence: 0,
                proposal: p.clone(),
            },
        )
    }
    pub fn reserve_execution(&self, id: &str, call: &str, key: &str) -> anyhow::Result<()> {
        self.conn.lock().unwrap().execute("UPDATE agent_turns SET active_request_id=?1 WHERE id=?2 AND pending_proposal_json IS NOT NULL",params![key,id])?;
        self.conn.lock().unwrap().execute("INSERT OR IGNORE INTO agent_tool_calls(turn_id,tool_call_id,idempotency_key,status) VALUES(?1,?2,?3,'reserved')",params![id,call,key])?;
        Ok(())
    }
    pub fn execution_request_id(&self, id: &str, call: &str) -> anyhow::Result<Option<String>> {
        Ok(self
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT request_id FROM agent_tool_calls WHERE turn_id=?1 AND tool_call_id=?2",
                params![id, call],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }
    pub fn record_receipt(
        &self,
        id: &str,
        p: &AgentToolProposal,
        r: &LabbyExecutionReceipt,
    ) -> anyhow::Result<()> {
        let turn = self
            .load(id)?
            .ok_or_else(|| anyhow::anyhow!("agent_turn_not_found"))?;
        let key: String = self.conn.lock().unwrap().query_row(
            "SELECT idempotency_key FROM agent_tool_calls WHERE turn_id=?1 AND tool_call_id=?2",
            params![id, p.tool_call_id],
            |row| row.get(0),
        )?;
        if r.execution_context_id != turn.execution_context_id
            || r.idempotency_key != key
            || r.tool_id != p.tool_id
            || r.contract_hash != p.contract_hash
            || r.loadout_id != turn.loadout_id
            || r.loadout_revision != turn.loadout_revision
            || r.actor != turn.actor
            || r.service != turn.service
            || r.request_id.is_empty()
            || r.receipt_id.is_empty()
            || r.audit_id.is_empty()
        {
            anyhow::bail!("labby_receipt_binding_mismatch");
        }
        self.conn.lock().unwrap().execute("UPDATE agent_tool_calls SET request_id=?1,receipt_id=?2,audit_id=?3,status=?4 WHERE turn_id=?5 AND tool_call_id=?6",params![r.request_id,r.receipt_id,r.audit_id,r.status,id,p.tool_call_id])?;
        self.append_event(
            id,
            AgentEvent::LabbyExecution {
                sequence: 0,
                request_id: r.request_id.clone(),
                receipt_id: r.receipt_id.clone(),
                audit_id: r.audit_id.clone(),
                status: r.status.clone(),
            },
        )
    }
    pub fn complete_tool(&self, id: &str, call: &str, result: Value) -> anyhow::Result<()> {
        let mut turn = self
            .load(id)?
            .ok_or_else(|| anyhow::anyhow!("agent_turn_not_found"))?;
        turn.tool_results
            .push(serde_json::json!({"toolCallId":call,"result":result}));
        self.conn.lock().unwrap().execute("UPDATE agent_turns SET status='continuing',tool_call_count=tool_call_count+1,pending_proposal_json=NULL,tool_results_json=?1,active_request_id=NULL WHERE id=?2",params![serde_json::to_string(&turn.tool_results)?,id])?;
        self.append_event(
            id,
            AgentEvent::ToolResult {
                sequence: 0,
                tool_call_id: call.into(),
                result,
            },
        )
    }
    pub fn append_event(&self, id: &str, event: AgentEvent) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let seq: i64 = conn.query_row(
            "UPDATE agent_turns SET sequence=sequence+1 WHERE id=?1 RETURNING sequence",
            [id],
            |r| r.get(0),
        )?;
        let event = with_sequence(event, seq as u64);
        conn.execute(
            "INSERT INTO agent_turn_events(turn_id,sequence,event_json) VALUES(?1,?2,?3)",
            params![id, seq, serde_json::to_string(&event)?],
        )?;
        Ok(())
    }
    pub fn events(&self, id: &str, after: u64) -> anyhow::Result<Vec<AgentEvent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt=conn.prepare("SELECT event_json FROM agent_turn_events WHERE turn_id=?1 AND sequence>?2 ORDER BY sequence LIMIT 1000")?;
        Ok(stmt
            .query_map(params![id, after as i64], |r| r.get::<_, String>(0))?
            .filter_map(|v| v.ok().and_then(|s| serde_json::from_str(&s).ok()))
            .collect())
    }
    pub fn result(&self, id: &str) -> anyhow::Result<AgentTurnResult> {
        let turn = self
            .load(id)?
            .ok_or_else(|| anyhow::anyhow!("agent_turn_not_found"))?;
        let conn = self.conn.lock().unwrap();
        let mut stmt=conn.prepare("SELECT request_id,receipt_id,audit_id FROM agent_tool_calls WHERE turn_id=?1 AND receipt_id IS NOT NULL ORDER BY rowid")?;
        let rows = stmt
            .query_map([id], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        Ok(AgentTurnResult {
            turn_id: id.into(),
            status: turn.status,
            answer: conn.query_row("SELECT answer FROM agent_turns WHERE id=?1", [id], |r| {
                r.get(0)
            })?,
            pending_approval: turn.pending_proposal,
            correlation: AgentCorrelation {
                turn_id: id.into(),
                execution_context_id: turn.execution_context_id,
                loadout_id: turn.loadout_id,
                loadout_revision: turn.loadout_revision,
                actor: turn.actor,
                service: turn.service,
                tool_call_count: turn.tool_call_count,
                request_ids: rows.iter().filter_map(|v| v.0.clone()).collect(),
                receipt_ids: rows.iter().filter_map(|v| v.1.clone()).collect(),
                audit_ids: rows.iter().filter_map(|v| v.2.clone()).collect(),
            },
        })
    }
}

fn digest(v: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(v.as_bytes())))
}
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}
fn status_str(s: &AgentTurnStatus) -> &'static str {
    match s {
        AgentTurnStatus::Pending => "pending",
        AgentTurnStatus::Proposing => "proposing",
        AgentTurnStatus::AwaitingApproval => "awaiting_approval",
        AgentTurnStatus::Executing => "executing",
        AgentTurnStatus::Continuing => "continuing",
        AgentTurnStatus::Succeeded => "succeeded",
        AgentTurnStatus::Failed => "failed",
        AgentTurnStatus::Cancelled => "cancelled",
        AgentTurnStatus::TimedOut => "timed_out",
        AgentTurnStatus::Interrupted => "interrupted",
        AgentTurnStatus::CancelUnconfirmed => "cancel_unconfirmed",
    }
}
fn parse_status(s: &str) -> AgentTurnStatus {
    match s {
        "pending" => AgentTurnStatus::Pending,
        "proposing" => AgentTurnStatus::Proposing,
        "awaiting_approval" => AgentTurnStatus::AwaitingApproval,
        "executing" => AgentTurnStatus::Executing,
        "continuing" => AgentTurnStatus::Continuing,
        "succeeded" => AgentTurnStatus::Succeeded,
        "cancelled" => AgentTurnStatus::Cancelled,
        "timed_out" => AgentTurnStatus::TimedOut,
        "interrupted" => AgentTurnStatus::Interrupted,
        "cancel_unconfirmed" => AgentTurnStatus::CancelUnconfirmed,
        _ => AgentTurnStatus::Failed,
    }
}
fn with_sequence(e: AgentEvent, s: u64) -> AgentEvent {
    match e {
        AgentEvent::State { status, .. } => AgentEvent::State {
            sequence: s,
            status,
        },
        AgentEvent::ModelProposal { proposal, .. } => AgentEvent::ModelProposal {
            sequence: s,
            proposal,
        },
        AgentEvent::AxonDecision { decision, .. } => AgentEvent::AxonDecision {
            sequence: s,
            decision,
        },
        AgentEvent::LabbyExecution {
            request_id,
            receipt_id,
            audit_id,
            status,
            ..
        } => AgentEvent::LabbyExecution {
            sequence: s,
            request_id,
            receipt_id,
            audit_id,
            status,
        },
        AgentEvent::ToolResult {
            tool_call_id,
            result,
            ..
        } => AgentEvent::ToolResult {
            sequence: s,
            tool_call_id,
            result,
        },
        AgentEvent::Final { answer, .. } => AgentEvent::Final {
            sequence: s,
            answer,
        },
    }
}

fn migrate(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(SCHEMA)?;
    for sql in [
        "ALTER TABLE agent_turns ADD COLUMN owner TEXT NOT NULL DEFAULT 'legacy'",
        "ALTER TABLE agent_turns ADD COLUMN profile_id TEXT NOT NULL DEFAULT 'legacy'",
        "ALTER TABLE agent_turns ADD COLUMN max_tool_calls INTEGER NOT NULL DEFAULT 8",
        "ALTER TABLE agent_turns ADD COLUMN model TEXT NOT NULL DEFAULT 'legacy'",
        "ALTER TABLE agent_turns ADD COLUMN version INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE agent_turns ADD COLUMN lease_until_ms INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE agent_turns ADD COLUMN cancel_requested INTEGER NOT NULL DEFAULT 0",
    ] {
        let _ = conn.execute(sql, []);
    }
    conn.pragma_update(None, "user_version", 1)?;
    Ok(())
}
const SCHEMA: &str = r#"CREATE TABLE IF NOT EXISTS agent_turns(id TEXT PRIMARY KEY,loadout_id TEXT NOT NULL,loadout_revision INTEGER NOT NULL,prompt TEXT NOT NULL,prompt_hash TEXT NOT NULL,execution_context_id TEXT NOT NULL,actor TEXT NOT NULL,service TEXT NOT NULL,status TEXT NOT NULL,deadline_at_ms INTEGER NOT NULL,tool_call_count INTEGER NOT NULL,pending_proposal_json TEXT,tool_results_json TEXT NOT NULL,active_request_id TEXT,answer TEXT,error_kind TEXT,sequence INTEGER NOT NULL,owner TEXT NOT NULL,profile_id TEXT NOT NULL,max_tool_calls INTEGER NOT NULL,model TEXT NOT NULL,version INTEGER NOT NULL DEFAULT 0,lease_until_ms INTEGER NOT NULL DEFAULT 0,cancel_requested INTEGER NOT NULL DEFAULT 0);CREATE TABLE IF NOT EXISTS agent_tool_calls(turn_id TEXT NOT NULL REFERENCES agent_turns(id) ON DELETE CASCADE,tool_call_id TEXT NOT NULL,idempotency_key TEXT NOT NULL,request_id TEXT,receipt_id TEXT,audit_id TEXT,status TEXT NOT NULL,PRIMARY KEY(turn_id,tool_call_id),UNIQUE(idempotency_key));CREATE TABLE IF NOT EXISTS agent_turn_events(turn_id TEXT NOT NULL REFERENCES agent_turns(id) ON DELETE CASCADE,sequence INTEGER NOT NULL,event_json TEXT NOT NULL,PRIMARY KEY(turn_id,sequence));"#;
