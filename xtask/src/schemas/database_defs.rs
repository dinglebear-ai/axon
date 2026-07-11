//! Real SQLite migration parser backing the `database` schema family.
//!
//! Walks every `*.sql` file under the store-owning crates' `migrations/`
//! directories (in filename order, which is also apply order) and builds a
//! snapshot of the resulting schema: tables (with columns, primary keys,
//! foreign keys), standalone indexes, and the migration file list itself.
//!
//! This is a pragmatic statement-level parser tuned to the SQL dialect these
//! migration files actually use (see `crates/axon-jobs/src/migrations`,
//! `crates/axon-ledger/src/migrations`, `crates/axon-graph/src/migrations`,
//! and `crates/axon-memory/src/migrations`), not a general SQL grammar.
//! Statement-level parsing primitives live in `database_defs/parser.rs` to
//! stay under the repo's 500-line file cap.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Value, json};

#[path = "database_defs/parser.rs"]
mod parser;

/// One migrations directory, tagged with the crate that owns it.
pub(super) struct MigrationSource {
    pub owner_crate: &'static str,
    pub dir: &'static str,
}

pub(super) const MIGRATION_SOURCES: &[MigrationSource] = &[
    MigrationSource {
        owner_crate: "axon-ledger",
        dir: "crates/axon-ledger/src/migrations",
    },
    MigrationSource {
        owner_crate: "axon-jobs",
        dir: "crates/axon-jobs/src/migrations",
    },
    MigrationSource {
        owner_crate: "axon-graph",
        dir: "crates/axon-graph/src/migrations",
    },
    MigrationSource {
        owner_crate: "axon-memory",
        dir: "crates/axon-memory/src/migrations",
    },
];

#[derive(Debug, Default)]
pub(super) struct DatabaseSchema {
    tables: BTreeMap<String, parser::Table>,
    indexes: BTreeMap<String, parser::IndexDef>,
    migrations: Vec<MigrationRecord>,
}

#[derive(Debug, Clone)]
struct MigrationRecord {
    file: String,
    owner_crate: &'static str,
    tables_touched: Vec<String>,
}

/// Parse every configured migrations directory under `root` and build the
/// merged final-state schema snapshot.
pub(super) fn parse_all(root: &Path) -> Result<DatabaseSchema> {
    let mut schema = DatabaseSchema::default();
    for source in MIGRATION_SOURCES {
        let dir = root.join(source.dir);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .with_context(|| format!("read migrations dir {}", dir.display()))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("sql"))
            .collect();
        files.sort();
        for file in files {
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("read migration {}", file.display()))?;
            let rel_name = format!(
                "{}/{}",
                source.dir,
                file.file_name().and_then(|n| n.to_str()).unwrap_or("")
            );
            let touched = apply_migration(&mut schema, source.owner_crate, &rel_name, &text);
            schema.migrations.push(MigrationRecord {
                file: rel_name,
                owner_crate: source.owner_crate,
                tables_touched: touched,
            });
        }
    }
    Ok(schema)
}

/// Apply one migration file's statements to the running schema snapshot,
/// returning the names of tables it touched (created, altered, or renamed).
fn apply_migration(
    schema: &mut DatabaseSchema,
    owner_crate: &'static str,
    file: &str,
    text: &str,
) -> Vec<String> {
    let mut touched = Vec::new();
    for raw_stmt in parser::split_statements(text) {
        let normalized = parser::normalize(&raw_stmt);
        let upper = normalized.to_uppercase();

        if upper.starts_with("CREATE TABLE") {
            if let Some(table) = parser::parse_create_table(&normalized, owner_crate, file) {
                touched.push(table.name.clone());
                schema.tables.insert(table.name.clone(), table);
            }
        } else if upper.starts_with("CREATE UNIQUE INDEX") || upper.starts_with("CREATE INDEX") {
            if let Some(index) = parser::parse_create_index(&normalized) {
                touched.push(index.table.clone());
                schema.indexes.insert(index.name.clone(), index);
            }
        } else if upper.starts_with("ALTER TABLE") {
            if let Some(name) = apply_alter_table(schema, &normalized) {
                touched.push(name);
            }
        } else if upper.starts_with("DROP TABLE") {
            if let Some(name) = parser::parse_drop_table(&normalized) {
                schema.tables.remove(&name);
                touched.push(name);
            }
        }
        // INSERT/SELECT/UPDATE/PRAGMA statements carry no schema-shape
        // information and are intentionally ignored.
    }
    touched.sort();
    touched.dedup();
    touched
}

/// Applies `ALTER TABLE t ADD COLUMN ...` and `ALTER TABLE t RENAME TO new`.
/// Returns the (post-rename) table name touched, if any.
fn apply_alter_table(schema: &mut DatabaseSchema, stmt: &str) -> Option<String> {
    let after = stmt.strip_prefix("ALTER TABLE ")?;
    let mut parts = after.splitn(2, char::is_whitespace);
    let table_name = parts.next()?.to_string();
    let rest = parts.next().unwrap_or("").trim();
    let rest_upper = rest.to_uppercase();

    if let Some(new_name_upper_idx) = rest_upper.find("RENAME TO") {
        let new_name = rest[new_name_upper_idx + "RENAME TO".len()..]
            .trim()
            .to_string();
        if let Some(mut table) = schema.tables.remove(&table_name) {
            table.name = new_name.clone();
            schema.tables.insert(new_name.clone(), table);
        }
        return Some(new_name);
    }

    if let Some(add_col_idx) = rest_upper.find("ADD COLUMN") {
        let col_def = rest[add_col_idx + "ADD COLUMN".len()..].trim();
        let column = parser::parse_column_def(col_def);
        if let Some(table) = schema.tables.get_mut(&table_name) {
            table.columns.retain(|c| c.name != column.name);
            table.columns.push(column);
        }
        return Some(table_name);
    }

    None
}

/// Known table-name divergences worth documenting rather than silently
/// "fixing" by renaming a live table (a separate contract decision, out of
/// scope for this generator).
fn divergences() -> Vec<Value> {
    vec![
        json!({
            "kind": "duplicate_domain_naming",
            "tables": ["axon_memory_nodes", "axon_memory_edges", "memory_records", "memory_links", "memory_reinforcement", "memory_reviews"],
            "note": "Two independent 'memory' schemas coexist: the legacy agent-memory graph (axon_memory_nodes/edges, crates/axon-jobs/src/migrations/0009_create_memory_tables.sql) and the current axon-memory durable store (memory_records/memory_links/memory_reinforcement/memory_reviews, crates/axon-memory/src/migrations/0001_create_memory_tables.sql). Not renamed here; documented for the future contract decision."
        }),
        json!({
            "kind": "legacy_per_family_job_tables",
            "tables": ["axon_crawl_jobs", "axon_embed_jobs", "axon_extract_jobs", "axon_ingest_jobs"],
            "note": "Predate the unified `jobs`/`job_attempts`/`job_stages`/`job_events` tables introduced in crates/axon-jobs/src/migrations/0018_unified_jobs_observability.sql. `embed`/`ingest`/`scrape`/`crawl` CLI/MCP/REST surfaces are already removed (see xtask/src/schemas/registry.rs::REMOVED_SURFACE_RULES); the legacy tables persist for `extract` and in-flight rows and are not renamed here."
        }),
        json!({
            "kind": "watch_naming_overlap",
            "tables": ["axon_watch_defs", "axon_watch_runs", "axon_source_watches", "axon_source_watch_runs"],
            "note": "axon_watch_defs/axon_watch_runs (migration 0002) back the task_type/task_payload watch scheduler; axon_source_watches/axon_source_watch_runs (migration 0023) back the newer SourceRequest-shaped WatchStore used by `watch get|update|pause|resume|delete`. Both are live; not merged here."
        }),
    ]
}

fn build_tables_field(schema: &DatabaseSchema) -> Vec<Value> {
    let mut tables: Vec<Value> = schema
        .tables
        .values()
        .map(|table| {
            let columns: Vec<Value> = table
                .columns
                .iter()
                .map(|column| {
                    json!({
                        "name": column.name,
                        "type": column.sql_type.to_lowercase(),
                        "nullable": column.nullable,
                        "primary_key": column.primary_key,
                    })
                })
                .collect();
            let foreign_keys: Vec<Value> = table
                .foreign_keys
                .iter()
                .map(|fk| {
                    json!({
                        "columns": fk.columns,
                        "references_table": fk.ref_table,
                        "references_columns": fk.ref_columns,
                        "on_delete": fk.on_delete,
                    })
                })
                .collect();
            json!({
                "name": table.name,
                "owner_crate": table.owner_crate,
                "introduced_in": table.introduced_in,
                "primary_key": table.primary_key,
                "columns": columns,
                "foreign_keys": foreign_keys,
            })
        })
        .collect();
    tables.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    tables
}

fn build_indexes_field(schema: &DatabaseSchema) -> Vec<Value> {
    let mut indexes: Vec<Value> = schema
        .indexes
        .values()
        .map(|index| {
            json!({
                "name": index.name,
                "table": index.table,
                "columns": index.columns,
                "unique": index.unique,
                "where": index.where_clause,
            })
        })
        .collect();
    indexes.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    indexes
}

fn build_foreign_keys_field(schema: &DatabaseSchema) -> Vec<Value> {
    let mut foreign_keys: Vec<Value> = Vec::new();
    for table in schema.tables.values() {
        for fk in &table.foreign_keys {
            foreign_keys.push(json!({
                "table": table.name,
                "columns": fk.columns,
                "references_table": fk.ref_table,
                "references_columns": fk.ref_columns,
                "on_delete": fk.on_delete,
            }));
        }
    }
    foreign_keys.sort_by(|a, b| {
        (a["table"].as_str(), a["columns"].to_string())
            .cmp(&(b["table"].as_str(), b["columns"].to_string()))
    });
    foreign_keys
}

fn build_migrations_field(schema: &DatabaseSchema) -> Vec<Value> {
    schema
        .migrations
        .iter()
        .map(|record| {
            json!({
                "file": record.file,
                "owner_crate": record.owner_crate,
                "tables_touched": record.tables_touched,
            })
        })
        .collect()
}

/// Build the `tables`/`indexes`/`foreign_keys`/`views`/`migrations`/
/// `divergences` root fields plus a small summary object for markdown
/// rendering.
pub(super) fn build_artifact_fields(root: &Path) -> Result<(Value, Value)> {
    let schema = parse_all(root)?;

    let tables = build_tables_field(&schema);
    let indexes = build_indexes_field(&schema);
    let foreign_keys = build_foreign_keys_field(&schema);
    let migrations = build_migrations_field(&schema);

    let root_fields = json!({
        "tables": tables,
        "indexes": indexes,
        "foreign_keys": foreign_keys,
        "views": [],
        "migrations": migrations,
        "divergences": divergences(),
    });

    let summary = json!({
        "table_count": schema.tables.len(),
        "index_count": schema.indexes.len(),
        "migration_count": schema.migrations.len(),
    });

    Ok((root_fields, summary))
}

#[cfg(test)]
#[path = "database_defs_tests.rs"]
mod tests;
