//! Neo4j schema setup (constraints, indexes) and Postgres graph job table.

use crate::crates::core::neo4j::Neo4jClient;
use crate::crates::jobs::common::begin_schema_migration_tx;
use sqlx::PgPool;

const GRAPH_SCHEMA_LOCK_KEY: i64 = 0xA804_0006;

/// Ensure the graph extraction job table exists in Postgres.
pub async fn ensure_graph_schema(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = begin_schema_migration_tx(pool, GRAPH_SCHEMA_LOCK_KEY).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS axon_graph_jobs (
            id             UUID PRIMARY KEY,
            url            TEXT NOT NULL,
            status         TEXT NOT NULL DEFAULT 'pending',
            chunk_count    INTEGER DEFAULT 0,
            entity_count   INTEGER DEFAULT 0,
            relation_count INTEGER DEFAULT 0,
            result_json    JSONB,
            config_json    JSONB,
            error_text     TEXT,
            created_at     TIMESTAMPTZ DEFAULT now(),
            updated_at     TIMESTAMPTZ DEFAULT now(),
            started_at     TIMESTAMPTZ,
            finished_at    TIMESTAMPTZ
        )
        "#,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query("ALTER TABLE axon_graph_jobs ADD COLUMN IF NOT EXISTS result_json JSONB")
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        r#"DO $$ BEGIN
            ALTER TABLE axon_graph_jobs ADD CONSTRAINT axon_graph_jobs_status_check
                CHECK (status IN ('pending', 'running', 'completed', 'failed', 'canceled'));
        EXCEPTION WHEN duplicate_object THEN NULL;
        END $$"#,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_graph_jobs_status ON axon_graph_jobs(status)")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_graph_jobs_status_created_desc \
         ON axon_graph_jobs(status, created_at DESC)",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_graph_jobs_url ON axon_graph_jobs(url)")
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

/// Ensure required Neo4j constraints and indexes exist.
pub async fn ensure_neo4j_schema(
    neo4j: &Neo4jClient,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let statements = [
        "CREATE CONSTRAINT entity_name IF NOT EXISTS FOR (e:Entity) REQUIRE e.name IS UNIQUE",
        "CREATE CONSTRAINT document_url IF NOT EXISTS FOR (d:Document) REQUIRE d.url IS UNIQUE",
        "CREATE INDEX chunk_point_id IF NOT EXISTS FOR (c:Chunk) ON (c.point_id)",
        "CREATE INDEX entity_type IF NOT EXISTS FOR (e:Entity) ON (e.entity_type)",
    ];

    for cypher in statements {
        neo4j.execute(cypher, serde_json::json!({})).await?;
    }

    Ok(())
}
