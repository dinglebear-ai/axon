use rusqlite::Connection;

pub(super) fn migrate(conn: &Connection) -> anyhow::Result<()> {
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
