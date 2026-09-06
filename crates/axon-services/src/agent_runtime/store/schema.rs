use rusqlite::Connection;

pub(super) fn migrate(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(SCHEMA)?;
    for (name, declaration) in [
        ("owner", "TEXT NOT NULL DEFAULT 'legacy'"),
        ("profile_id", "TEXT NOT NULL DEFAULT 'legacy'"),
        ("max_tool_calls", "INTEGER NOT NULL DEFAULT 8"),
        ("model", "TEXT NOT NULL DEFAULT 'legacy'"),
        ("version", "INTEGER NOT NULL DEFAULT 0"),
        ("lease_until_ms", "INTEGER NOT NULL DEFAULT 0"),
        ("cancel_requested", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !column_exists(conn, name)? {
            conn.execute(
                &format!("ALTER TABLE agent_turns ADD COLUMN {name} {declaration}"),
                [],
            )?;
        }
    }
    if !column_exists_in(conn, "agent_tool_calls", "proposal_digest")? {
        conn.execute(
            "ALTER TABLE agent_tool_calls ADD COLUMN proposal_digest TEXT",
            [],
        )?;
    }
    verify_columns(conn)?;
    conn.pragma_update(None, "user_version", 1)?;
    Ok(())
}

fn column_exists(conn: &Connection, expected: &str) -> anyhow::Result<bool> {
    column_exists_in(conn, "agent_turns", expected)
}

fn column_exists_in(conn: &Connection, table: &str, expected: &str) -> anyhow::Result<bool> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = statement.query_map([], |row| row.get::<_, String>(1))?;
    for name in names {
        if name? == expected {
            return Ok(true);
        }
    }
    Ok(false)
}

fn verify_columns(conn: &Connection) -> anyhow::Result<()> {
    for name in [
        "owner",
        "profile_id",
        "max_tool_calls",
        "model",
        "version",
        "lease_until_ms",
        "cancel_requested",
    ] {
        if !column_exists(conn, name)? {
            anyhow::bail!("agent_store_schema_missing_column:{name}");
        }
    }
    Ok(())
}

const SCHEMA: &str = r#"CREATE TABLE IF NOT EXISTS agent_turns(id TEXT PRIMARY KEY,loadout_id TEXT NOT NULL,loadout_revision INTEGER NOT NULL,prompt TEXT NOT NULL,prompt_hash TEXT NOT NULL,execution_context_id TEXT NOT NULL,actor TEXT NOT NULL,service TEXT NOT NULL,status TEXT NOT NULL,deadline_at_ms INTEGER NOT NULL,tool_call_count INTEGER NOT NULL,pending_proposal_json TEXT,tool_results_json TEXT NOT NULL,active_request_id TEXT,answer TEXT,error_kind TEXT,sequence INTEGER NOT NULL,owner TEXT NOT NULL,profile_id TEXT NOT NULL,max_tool_calls INTEGER NOT NULL,model TEXT NOT NULL,version INTEGER NOT NULL DEFAULT 0,lease_until_ms INTEGER NOT NULL DEFAULT 0,cancel_requested INTEGER NOT NULL DEFAULT 0);CREATE TABLE IF NOT EXISTS agent_tool_calls(turn_id TEXT NOT NULL REFERENCES agent_turns(id) ON DELETE CASCADE,tool_call_id TEXT NOT NULL,idempotency_key TEXT NOT NULL,proposal_digest TEXT,request_id TEXT,receipt_id TEXT,audit_id TEXT,status TEXT NOT NULL,PRIMARY KEY(turn_id,tool_call_id),UNIQUE(idempotency_key));CREATE TABLE IF NOT EXISTS agent_turn_events(turn_id TEXT NOT NULL REFERENCES agent_turns(id) ON DELETE CASCADE,sequence INTEGER NOT NULL,event_json TEXT NOT NULL,PRIMARY KEY(turn_id,sequence));"#;

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
