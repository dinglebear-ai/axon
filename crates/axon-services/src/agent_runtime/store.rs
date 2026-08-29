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
}

impl StoredTurn {
    pub fn verify_resume(&self, loadout: &str, revision: u64, prompt: &str) -> anyhow::Result<()> {
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
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
    #[cfg(test)]
    pub fn memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
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
        context: &LabbyContextReceipt,
    ) -> anyhow::Result<StoredTurn> {
        self.conn.lock().unwrap().execute("INSERT INTO agent_turns(id,loadout_id,loadout_revision,prompt,prompt_hash,execution_context_id,actor,service,status,deadline_at_ms,tool_call_count,tool_results_json,sequence) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'pending',?9,0,'[]',0)",params![id,loadout,revision as i64,prompt,digest(prompt),context.execution_context_id,context.actor,context.service,deadline])?;
        self.load(id)?
            .ok_or_else(|| anyhow::anyhow!("agent_turn_create_failed"))
    }
    pub fn load(&self, id: &str) -> anyhow::Result<Option<StoredTurn>> {
        self.conn.lock().unwrap().query_row("SELECT id,loadout_id,loadout_revision,prompt,prompt_hash,execution_context_id,actor,service,status,deadline_at_ms,tool_call_count,pending_proposal_json,tool_results_json,active_request_id FROM agent_turns WHERE id=?1",[id],|r|{
            let status:String=r.get(8)?; let proposal:Option<String>=r.get(11)?; let results:String=r.get(12)?;
            Ok(StoredTurn{id:r.get(0)?,loadout_id:r.get(1)?,loadout_revision:r.get::<_,i64>(2)? as u64,prompt:r.get(3)?,prompt_hash:r.get(4)?,execution_context_id:r.get(5)?,actor:r.get(6)?,service:r.get(7)?,status:parse_status(&status),deadline_at_ms:r.get(9)?,tool_call_count:r.get::<_,i64>(10)? as u32,pending_proposal:proposal.and_then(|v|serde_json::from_str(&v).ok()),tool_results:serde_json::from_str(&results).unwrap_or_default(),active_request_id:r.get(13)?})
        }).optional().map_err(Into::into)
    }
    pub fn transition(
        &self,
        id: &str,
        status: AgentTurnStatus,
        value: Option<&str>,
    ) -> anyhow::Result<()> {
        self.conn.lock().unwrap().execute("UPDATE agent_turns SET status=?1,answer=CASE WHEN ?2 IS NULL THEN answer ELSE ?2 END,error_kind=CASE WHEN ?3 THEN ?2 ELSE error_kind END WHERE id=?4",params![status_str(&status),value,!matches!(status,AgentTurnStatus::Succeeded),id])?;
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
                "SELECT idempotency_key FROM agent_tool_calls WHERE turn_id=?1 AND tool_call_id=?2",
                params![id, call],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub fn record_receipt(
        &self,
        id: &str,
        p: &AgentToolProposal,
        r: &LabbyExecutionReceipt,
    ) -> anyhow::Result<()> {
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

const SCHEMA: &str = r#"CREATE TABLE IF NOT EXISTS agent_turns(id TEXT PRIMARY KEY,loadout_id TEXT NOT NULL,loadout_revision INTEGER NOT NULL,prompt TEXT NOT NULL,prompt_hash TEXT NOT NULL,execution_context_id TEXT NOT NULL,actor TEXT NOT NULL,service TEXT NOT NULL,status TEXT NOT NULL,deadline_at_ms INTEGER NOT NULL,tool_call_count INTEGER NOT NULL,pending_proposal_json TEXT,tool_results_json TEXT NOT NULL,active_request_id TEXT,answer TEXT,error_kind TEXT,sequence INTEGER NOT NULL);CREATE TABLE IF NOT EXISTS agent_tool_calls(turn_id TEXT NOT NULL,tool_call_id TEXT NOT NULL,idempotency_key TEXT NOT NULL,request_id TEXT,receipt_id TEXT,audit_id TEXT,status TEXT NOT NULL,PRIMARY KEY(turn_id,tool_call_id),UNIQUE(idempotency_key));CREATE TABLE IF NOT EXISTS agent_turn_events(turn_id TEXT NOT NULL,sequence INTEGER NOT NULL,event_json TEXT NOT NULL,PRIMARY KEY(turn_id,sequence));"#;
