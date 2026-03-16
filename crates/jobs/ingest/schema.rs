use crate::crates::jobs::common::begin_schema_migration_tx;
use sqlx::PgPool;

/// Advisory lock key for ingest job schema migrations (unique per table).
const INGEST_SCHEMA_LOCK_KEY: i64 = 0x696e_6765_7374_0000_i64;

pub(crate) async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = begin_schema_migration_tx(pool, INGEST_SCHEMA_LOCK_KEY).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS axon_ingest_jobs (
            id          UUID PRIMARY KEY,
            status      TEXT NOT NULL CHECK (status IN ('pending', 'running', 'completed', 'failed', 'canceled')),
            source_type TEXT NOT NULL,
            target      TEXT NOT NULL,
            created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            started_at  TIMESTAMPTZ,
            finished_at TIMESTAMPTZ,
            error_text  TEXT,
            result_json JSONB,
            config_json JSONB NOT NULL
        )
        "#,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_axon_ingest_jobs_pending \
         ON axon_ingest_jobs(created_at ASC) WHERE status = 'pending'",
    )
    .execute(&mut *tx)
    .await?;

    // Partial index for stale-job watchdog sweeps (WHERE status='running' AND updated_at < threshold).
    // Without this, reclaim_stale_running_jobs does a full table scan that blocks heartbeat UPDATEs.
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_axon_ingest_jobs_running_updated \
         ON axon_ingest_jobs(updated_at ASC) WHERE status = 'running'",
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"DO $$ BEGIN
            ALTER TABLE axon_ingest_jobs ADD CONSTRAINT axon_ingest_jobs_status_check
                CHECK (status IN ('pending', 'running', 'completed', 'failed', 'canceled'));
        EXCEPTION WHEN duplicate_object THEN NULL;
        END $$"#,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}
