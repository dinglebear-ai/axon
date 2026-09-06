use super::*;

impl AgentTurnStore {
    pub fn reserve_execution_fenced(
        &self,
        id: &str,
        version: u64,
        call: &str,
        key: &str,
    ) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let proposal_json: String = tx.query_row(
            "SELECT pending_proposal_json FROM agent_turns WHERE id=?1",
            [id],
            |row| row.get(0),
        )?;
        let proposal_digest = digest(&proposal_json);
        let changed=tx.execute("UPDATE agent_turns SET active_request_id=NULL WHERE id=?1 AND version=?2 AND lease_until_ms>?3 AND cancel_requested=0 AND pending_proposal_json IS NOT NULL",params![id,version as i64,now_ms()])?;
        if changed != 1 {
            anyhow::bail!("agent_turn_lease_lost");
        }
        tx.execute("INSERT OR IGNORE INTO agent_tool_calls(turn_id,tool_call_id,idempotency_key,proposal_digest,status) VALUES(?1,?2,?3,?4,'reserved')",params![id,call,key,proposal_digest])?;
        let binding: (String, Option<String>) = tx.query_row(
            "SELECT idempotency_key,proposal_digest FROM agent_tool_calls WHERE turn_id=?1 AND tool_call_id=?2",
            params![id, call],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if binding.0 != key || binding.1.as_deref() != Some(proposal_digest.as_str()) {
            anyhow::bail!("agent_tool_call_collision");
        }
        tx.commit()?;
        Ok(())
    }
    pub fn execution_request_id(&self, id: &str, call: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let current: Option<String> = conn
            .query_row(
                "SELECT pending_proposal_json FROM agent_turns WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let row: Option<(Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT request_id,proposal_digest FROM agent_tool_calls WHERE turn_id=?1 AND tool_call_id=?2",
                params![id, call],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match (current, row) {
            (Some(proposal), Some((request_id, Some(stored)))) if digest(&proposal) == stored => {
                Ok(request_id)
            }
            (_, Some(_)) => anyhow::bail!("agent_tool_call_collision"),
            (_, None) => Ok(None),
        }
    }
    pub fn record_receipt_fenced(
        &self,
        id: &str,
        version: u64,
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
        if key.is_empty()
            || r.tool_id != p.tool_id
            || r.contract_hash != p.contract_hash
            || r.loadout_id != turn.loadout_id
            || r.loadout_revision != turn.loadout_revision
            || r.actor != turn.actor
            || r.service != turn.service
            || r.execution_mode != "exact"
            || r.llm_invocations != 0
            || r.request_id.is_empty()
            || r.receipt_id.is_empty()
            || r.audit_id.is_empty()
        {
            anyhow::bail!("labby_receipt_binding_mismatch");
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let changed=tx.execute("UPDATE agent_tool_calls SET request_id=?1,receipt_id=?2,audit_id=?3,status=?4 WHERE turn_id=?5 AND tool_call_id=?6 AND EXISTS(SELECT 1 FROM agent_turns WHERE id=?5 AND version=?7 AND lease_until_ms>?8 AND cancel_requested=0)",params![r.request_id,r.receipt_id,r.audit_id,r.status,id,p.tool_call_id,version as i64,now_ms()])?;
        if changed != 1 {
            anyhow::bail!("agent_turn_lease_lost");
        }
        tx.execute("UPDATE agent_turns SET active_request_id=?1 WHERE id=?2 AND version=?3 AND lease_until_ms>?4 AND cancel_requested=0",params![r.request_id,id,version as i64,now_ms()])?;
        append_event_tx(
            &tx,
            id,
            AgentEvent::LabbyExecution {
                sequence: 0,
                request_id: r.request_id.clone(),
                receipt_id: r.receipt_id.clone(),
                audit_id: r.audit_id.clone(),
                status: r.status.clone(),
            },
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn complete_tool_fenced(
        &self,
        id: &str,
        version: u64,
        call: &str,
        result: Value,
    ) -> anyhow::Result<()> {
        let result = redact_agent_json(result)?;
        let mut turn = self
            .load(id)?
            .ok_or_else(|| anyhow::anyhow!("agent_turn_not_found"))?;
        turn.tool_results
            .push(serde_json::json!({"toolCallId":call,"result":result}));
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let changed=tx.execute("UPDATE agent_turns SET status='continuing',tool_call_count=tool_call_count+1,pending_proposal_json=NULL,tool_results_json=?1,active_request_id=NULL WHERE id=?2 AND version=?3 AND lease_until_ms>?4 AND cancel_requested=0",params![serde_json::to_string(&turn.tool_results)?,id,version as i64,now_ms()])?;
        if changed != 1 {
            anyhow::bail!("agent_turn_lease_lost");
        }
        append_event_tx(
            &tx,
            id,
            AgentEvent::ToolResult {
                sequence: 0,
                tool_call_id: call.into(),
                result,
            },
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn append_event_fenced(
        &self,
        id: &str,
        version: u64,
        event: AgentEvent,
    ) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM agent_turns WHERE id=?1 AND version=?2 AND lease_until_ms>?3 AND cancel_requested=0)",params![id,version as i64,now_ms()],|r|r.get(0))?;
        if !valid {
            anyhow::bail!("agent_turn_lease_lost");
        }
        append_event_tx(&tx, id, event)?;
        tx.commit()?;
        Ok(())
    }

    pub fn reconcile_dispatched_request(
        &self,
        id: &str,
        call: &str,
        key: &str,
        request_id: &str,
    ) -> anyhow::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let changed=conn.execute("UPDATE agent_tool_calls SET request_id=?1 WHERE turn_id=?2 AND tool_call_id=?3 AND idempotency_key=?4 AND (request_id IS NULL OR request_id=?1)",params![request_id,id,call,key])?;
        conn.execute("UPDATE agent_turns SET active_request_id=?1,status=CASE WHEN cancel_requested=1 THEN 'cancel_unconfirmed' ELSE status END WHERE id=?2 AND EXISTS(SELECT 1 FROM agent_tool_calls WHERE turn_id=?2 AND tool_call_id=?3 AND idempotency_key=?4 AND request_id=?1)",params![request_id,id,call,key])?;
        Ok(changed == 1)
    }

    pub fn append_event(&self, id: &str, event: AgentEvent) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        append_event_tx(&conn, id, event)
    }
    pub fn events(&self, id: &str, after: u64) -> anyhow::Result<Vec<AgentEvent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt=conn.prepare("SELECT event_json FROM agent_turn_events WHERE turn_id=?1 AND sequence>?2 ORDER BY sequence LIMIT 1000")?;
        stmt.query_map(params![id, after as i64], |r| r.get::<_, String>(0))?
            .map(|value| {
                let encoded = value?;
                serde_json::from_str(&encoded)
                    .map_err(|error| anyhow::anyhow!("agent_event_corrupt turn_id={id}: {error}"))
            })
            .collect()
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
            .collect::<Result<Vec<_>, _>>()?;
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
