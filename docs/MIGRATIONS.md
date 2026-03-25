# Database Migration Strategy

**Tracking issue:** A-M-04
**Status:** Scaffolding only — no migration tool adopted yet
**Last updated:** 2026-03-04

---

## Current State

Database schema is managed by inline `ensure_schema()` functions within each job module. Each job type (crawl, extract, embed, ingest, refresh) runs its own `CREATE TABLE IF NOT EXISTS` DDL on first use, protected by a PostgreSQL advisory lock.

Location of inline DDL:

| Table(s) | Source file |
|----------|-------------|
| `axon_crawl_jobs` | `crates/jobs/crawl/runtime.rs` |
| `axon_extract_jobs` | `crates/jobs/extract.rs` |
| `axon_embed_jobs` | `crates/jobs/embed.rs` |
| `axon_ingest_jobs` | `crates/jobs/ingest/schema.rs` |
| `axon_refresh_jobs`, `axon_refresh_targets`, `axon_refresh_schedules` | `crates/jobs/refresh.rs` |
| `axon_watch_defs`, `axon_watch_runs`, `axon_watch_run_artifacts` | `crates/jobs/watch.rs` |

A reference migration file has been created at:

```
migrations/001_initial_schema.sql
```

This file consolidates all `CREATE TABLE IF NOT EXISTS` statements as a baseline for when a proper migration tool is adopted. It is not executed by the application — it is documentation of the target state.

---

## Problems with Inline DDL

1. **No version history.** There is no way to know what schema version a database is running.
2. **ALTER TABLE is not tracked.** Any `ALTER TABLE` to add a column must be applied manually — there is no mechanism to replay it on existing databases.
3. **Schema drift.** A fresh database created today may differ from one created 6 months ago if `ALTER TABLE` statements were added to ensure_schema() ad-hoc.
4. **No rollback path.** Inline DDL does not support rollback to a previous schema version.
5. **Duplicate advisory lock constants.** Each module picks its own advisory lock key. There is no central registry to prevent collisions.

---

## Recommended Solution: sqlx-migrate

[sqlx-migrate](https://docs.rs/sqlx/latest/sqlx/migrate/) is the natural choice because:
- Already a dependency (`sqlx = "0.8"` in Cargo.toml)
- Migrations are plain `.sql` files in the `migrations/` directory
- Version-tracked via the `_sqlx_migrations` table in Postgres
- Supports rollback via `down` migration files
- Async-native — runs in tokio context

### Alternative: refinery

[refinery](https://docs.rs/refinery/) is a standalone migration runner that supports multiple backends (including sqlx) and does not require a specific version of sqlx. Suitable if sqlx major version is being upgraded.

---

## Adoption Steps

### Step 1: Add sqlx migrate feature

In `Cargo.toml`, the `sqlx` dependency already exists. Enable the `migrate` feature:

```toml
sqlx = { version = "0.8", features = [
    "runtime-tokio-rustls",
    "postgres",
    "uuid",
    "chrono",
    "migrate",          # ADD THIS
] }
```

### Step 2: Create migration files

The `migrations/` directory already exists with `001_initial_schema.sql` as the baseline. Subsequent schema changes (new columns, new tables, index modifications) become numbered `.sql` files:

```
migrations/
├── 001_initial_schema.sql           ← baseline (all tables from ensure_schema())
├── 002_add_crawl_jobs_crawl_id.sql  ← example: add crawl_id column
└── 003_ingest_jobs_add_index.sql    ← example: add index on source_type
```

### Step 3: Run migrations at startup

In `main.rs` (or in a shared startup function), run migrations before any worker or job handler starts:

```rust
use sqlx::migrate::MigrateDatabase;

async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
}
```

Call this once per process startup, before `ensure_schema()` calls. Once all ensure_schema() functions are removed, this becomes the sole schema initialization path.

### Step 4: Remove inline ensure_schema() functions

After migrations are running in production and all existing databases have been migrated:

1. Remove `ensure_schema()` from each job module
2. Remove `begin_schema_migration_tx()` (no longer needed)
3. Remove advisory lock constants
4. Remove `SCHEMA_INIT: OnceLock<()>` cells

### Step 5: CI verification

Add to CI:

```bash
# Verify all pending migrations apply cleanly to a fresh database
sqlx migrate run --database-url postgresql://axon:postgres@localhost:5432/axon_test
```

---

## Advisory Lock Key Registry

Current advisory lock keys in use (prevent collisions when adding new tables):

| Module | Constant | Hex |
|--------|----------|-----|
| crawl | `CRAWL_SCHEMA_LOCK_KEY` | `0x6372_6177_6c00_0000` |
| extract | `EXTRACT_SCHEMA_LOCK_KEY` | `0x6578_7472_6163_7400` |
| embed | `EMBED_SCHEMA_LOCK_KEY` | `0xA804_0002` |
| ingest | `INGEST_SCHEMA_LOCK_KEY` | `0x696e_6765_7374_0000` |
| refresh | *(none — runs without advisory lock)* | — |

When adding a new job table with inline DDL, choose a key not in this list and add it here.

---

## Notes

- All timestamps use `TIMESTAMPTZ` — always UTC-aware. Never use plain `TIMESTAMP`.
- The `status` column uses a `CHECK` constraint with five values: `pending`, `running`, `completed`, `failed`, `canceled`. This is enforced in both DDL and application code via `JobStatus` enum.
- `axon_ingest_jobs` differs from other job tables: it uses `source_type + target` instead of `url` or `urls_json`.
