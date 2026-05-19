# GraphRAG: Knowledge Graph Integration — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract entities and relationships from indexed content into Neo4j, then use graph traversal at query time to enrich `axon ask` results with structured knowledge graph context.

**Architecture:** Three-layer extraction pipeline (regex/taxonomy → embedding similarity → LLM) writes entities/relationships to Neo4j. `axon ask --graph` injects 1-hop graph context into the LLM prompt alongside vector results. All graph features are opt-in — gated behind `AXON_NEO4J_URL` being set.

**Tech Stack:** Rust, reqwest (Neo4j HTTP API), Qdrant recommend API, Ollama (Qwen 3.5 2B), lapin (AMQP), sqlx (Postgres), serde_json.

**Spec:** `docs/superpowers/specs/2026-03-10-project-linking-design.md`

---

## Scope Check

This is a single cohesive feature. All components (Neo4j client, extraction layers, graph commands, ask enrichment) share the same Neo4j client and taxonomy — they must be built together. However, the tasks are structured so each produces compilable, testable code independently.

## File Structure

```
crates/
├── core/
│   ├── neo4j.rs                          # NEW — thin HTTP client for Neo4j Cypher API
│   └── config/
│       └── types/
│           ├── config.rs                  # MODIFY — add neo4j_*, graph_* fields
│           ├── config_impls.rs            # MODIFY — add defaults
│           └── enums.rs                   # MODIFY — add CommandKind::Graph
├── cli/commands/
│   ├── commands.rs                        # MODIFY — add graph module + re-export
│   └── graph.rs                           # NEW — CLI handler: build/status/explore/stats/worker
├── services/
│   ├── graph.rs                           # NEW — service layer for graph operations
│   └── types/
│       └── service.rs                     # MODIFY — add graph result types
├── jobs/
│   ├── common.rs                          # MODIFY — add JobTable::Graph
│   ├── graph.rs                           # NEW — graph job module root (ensure_schema, enqueue, worker)
│   └── graph/
│       ├── taxonomy.rs                    # NEW — Layer 1: regex + curated tech taxonomy NER
│       ├── similarity.rs                  # NEW — Layer 2: Qdrant recommend API similarity edges
│       ├── extract.rs                     # NEW — Layer 3: LLM entity extraction via Ollama
│       ├── context.rs                     # NEW — GraphContext builder for ask --graph
│       ├── worker.rs                      # NEW — AMQP graph worker loop
│       └── schema.rs                      # NEW — Neo4j constraint/index setup
├── vector/ops/
│   └── qdrant/
│       └── types.rs                       # MODIFY — add `id` field to QdrantPoint, QdrantSearchHit
├── mcp/
│   ├── schema.rs                          # MODIFY — add Graph action
│   └── server/
│       └── handlers_graph.rs              # NEW — MCP graph handler
└── lib.rs                                 # MODIFY — add Graph dispatch
```

---

## Chunk 1: Foundation — Neo4j Client, Config, Qdrant Types

### Task 1: Neo4j HTTP Client (`crates/core/neo4j.rs`)

Thin `reqwest`-based client for Neo4j's HTTP transactional Cypher endpoint. No new crates — reuses the existing `reqwest` dependency. Returns `None` from constructor when `neo4j_url` is empty (feature disabled).

**Files:**
- Create: `crates/core/neo4j.rs`
- Modify: `crates/core/mod.rs` (if exists) or wherever core modules are declared

**Key context:**
- Neo4j HTTP API: `POST /db/neo4j/tx/commit` with `{"statements": [{"statement": "...", "parameters": {...}}]}`
- Auth: Basic auth header from `neo4j_user` + `neo4j_password`
- Use `crate::crates::core::http::http_client` for the HTTP client singleton? No — Neo4j needs its own client with auth headers baked in. Use `reqwest::Client` configured once in `Neo4jClient::new()`.
- All Cypher queries MUST use `$variable` parameters — no string interpolation of user input (injection prevention).

- [ ] **Step 1: Write tests for `Neo4jClient`**

Create `crates/core/neo4j.rs` with test module. Tests should cover:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_returns_none_when_url_empty() {
        let client = Neo4jClient::from_parts("", "neo4j", "");
        assert!(client.is_none());
    }

    #[test]
    fn new_returns_some_when_url_set() {
        let client = Neo4jClient::from_parts("http://localhost:7474", "neo4j", "pass");
        assert!(client.is_some());
    }

    #[test]
    fn build_request_body_single_statement() {
        let body = build_request_body("RETURN 1", serde_json::json!({}));
        let stmts = body["statements"].as_array().unwrap();
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0]["statement"], "RETURN 1");
    }

    #[test]
    fn build_request_body_with_params() {
        let params = serde_json::json!({"name": "Tokio"});
        let body = build_request_body("MATCH (e:Entity {name: $name}) RETURN e", params.clone());
        assert_eq!(body["statements"][0]["parameters"], params);
    }

    #[test]
    fn auth_header_built_correctly() {
        let client = Neo4jClient::from_parts("http://localhost:7474", "neo4j", "secret").unwrap();
        // Verify auth is set (we can check the endpoint and user fields)
        assert_eq!(client.endpoint, "http://localhost:7474/db/neo4j/tx/commit");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test neo4j -p axon_cli --lib 2>&1 | tail -20`
Expected: FAIL — `Neo4jClient` not defined

- [ ] **Step 3: Implement `Neo4jClient`**

```rust
//! Thin HTTP client for Neo4j's Cypher transactional endpoint.
//!
//! All queries use parameterized `$variables` — no string interpolation.
//! Returns `None` from `from_parts()` when the URL is empty (graph features disabled).

use base64::Engine;
use serde_json::Value;

pub struct Neo4jClient {
    http: reqwest::Client,
    pub(crate) endpoint: String,
    auth_header: Option<String>,
}

fn build_request_body(cypher: &str, params: Value) -> Value {
    serde_json::json!({
        "statements": [{
            "statement": cypher,
            "parameters": params
        }]
    })
}

impl Neo4jClient {
    /// Create a client from raw parts. Returns `None` if `url` is empty.
    pub fn from_parts(url: &str, user: &str, password: &str) -> Option<Self> {
        let url = url.trim();
        if url.is_empty() {
            return None;
        }
        let endpoint = format!("{}/db/neo4j/tx/commit", url.trim_end_matches('/'));
        let auth_header = if !password.is_empty() {
            let encoded = base64::engine::general_purpose::STANDARD
                .encode(format!("{user}:{password}"));
            Some(format!("Basic {encoded}"))
        } else {
            None
        };
        Some(Self {
            http: reqwest::Client::new(),
            endpoint,
            auth_header,
        })
    }

    /// Create from Config fields. Returns `None` if neo4j_url is empty.
    pub fn from_config(cfg: &crate::crates::core::config::Config) -> Option<Self> {
        Self::from_parts(&cfg.neo4j_url, &cfg.neo4j_user, &cfg.neo4j_password)
    }

    /// Execute a Cypher statement (no return data expected).
    pub async fn execute(&self, cypher: &str, params: Value) -> Result<(), Box<dyn std::error::Error>> {
        let body = build_request_body(cypher, params);
        let mut req = self.http.post(&self.endpoint).json(&body);
        if let Some(auth) = &self.auth_header {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let json: Value = resp.json().await?;
        if let Some(errors) = json["errors"].as_array() {
            if !errors.is_empty() {
                return Err(format!("Neo4j error: {}", errors[0]["message"]).into());
            }
        }
        if !status.is_success() {
            return Err(format!("Neo4j HTTP {status}").into());
        }
        Ok(())
    }

    /// Query Neo4j and return result rows.
    pub async fn query(&self, cypher: &str, params: Value) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let body = build_request_body(cypher, params);
        let mut req = self.http.post(&self.endpoint).json(&body);
        if let Some(auth) = &self.auth_header {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().await?;
        let json: Value = resp.json().await?;
        if let Some(errors) = json["errors"].as_array() {
            if !errors.is_empty() {
                return Err(format!("Neo4j error: {}", errors[0]["message"]).into());
            }
        }
        let rows = json["results"]
            .as_array()
            .and_then(|r| r.first())
            .and_then(|r| r["data"].as_array())
            .cloned()
            .unwrap_or_default();
        Ok(rows)
    }

    /// Health check — returns true if Neo4j responds.
    pub async fn health(&self) -> bool {
        self.execute("RETURN 1", serde_json::json!({})).await.is_ok()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test neo4j -p axon_cli --lib 2>&1 | tail -20`
Expected: 5 tests PASS

- [ ] **Step 5: Register the module**

Add `pub mod neo4j;` to the core module declarations. Find where `core` sub-modules are declared and add the new module.

- [ ] **Step 6: Commit**

```bash
git add crates/core/neo4j.rs
git commit -m "feat(core): add Neo4j HTTP Cypher client"
```

---

### Task 2: Config Fields for Graph Features

Add all graph-related configuration to the `Config` struct. This is a cross-cutting change that touches multiple files.

**Files:**
- Modify: `crates/core/config/types/config.rs` — add fields
- Modify: `crates/core/config/types/config_impls.rs` — add defaults
- Modify: `crates/core/config/types/enums.rs` — add `CommandKind::Graph`
- Modify: `crates/core/config/parse/build_config.rs` — env var loading
- Modify: `crates/cli/commands/research.rs` — update `make_test_config()` inline literal
- Modify: `crates/cli/commands/search.rs` — update `make_test_config()` inline literal
- Modify: `crates/jobs/common.rs` — update `test_config()` inline literal + add `JobTable::Graph`

**Critical gotcha:** The `Config` struct uses inline struct literals in test helpers. Adding non-`Option` fields breaks these at test compile time. Use `Option<T>` or `String` with empty defaults to minimize breakage — or update ALL inline literals.

- [ ] **Step 1: Add `CommandKind::Graph` to enums.rs**

Add the variant and its `as_str()` match arm:

```rust
// In CommandKind enum:
Graph,

// In as_str():
Self::Graph => "graph",
```

- [ ] **Step 2: Add config fields to `config.rs`**

Add these fields to the `Config` struct (all have sensible defaults):

```rust
// Neo4j
pub neo4j_url: String,
pub neo4j_user: String,
pub neo4j_password: String,

// Graph extraction
pub graph_queue: String,
pub graph_concurrency: usize,
pub graph_llm_url: String,
pub graph_llm_model: String,
pub graph_similarity_threshold: f64,
pub graph_similarity_limit: usize,
pub graph_context_max_chars: usize,
pub graph_taxonomy_path: String,

// Ask --graph flag
pub ask_graph: bool,
```

**Note:** The spec lists a `graph_enabled` field — this is intentionally omitted. The `!neo4j_url.is_empty()` guard serves the same purpose without a separate config field. Every graph code path already checks `neo4j_url` directly.

- [ ] **Step 2b: Add `Graph` variant to `CliCommand` in `crates/core/config/cli.rs`**

Add to the `CliCommand` clap enum:

```rust
/// Knowledge graph operations
Graph(TextArg),
```

And in `build_config.rs`, add the mapping:

```rust
CliCommand::Graph(args) => (CommandKind::Graph, args.text_as_positional()),
```

Also add `--graph` flag to `GlobalArgs`:

```rust
#[arg(long, global = true, help = "Enable graph-enhanced retrieval (requires Neo4j)")]
pub graph: bool,
```

And map it in `into_config()`:

```rust
ask_graph: cli.global.graph,
```

- [ ] **Step 2c: Add graph queue constant to `crates/jobs/common/amqp.rs`**

Following the pattern of existing queue constants:

```rust
pub const GRAPH_QUEUE_DEFAULT: &str = "axon.graph.jobs";
```

- [ ] **Step 3: Add defaults to `config_impls.rs`**

In `Config::default()`:

```rust
neo4j_url: String::new(),
neo4j_user: "neo4j".to_string(),
neo4j_password: String::new(),
graph_queue: "axon.graph.jobs".to_string(),
graph_concurrency: 4,
graph_llm_url: "http://localhost:11434".to_string(),
graph_llm_model: "qwen3.5:2b".to_string(),
graph_similarity_threshold: 0.75,
graph_similarity_limit: 20,
graph_context_max_chars: 2000,
graph_taxonomy_path: String::new(),
ask_graph: false,
```

Also add `neo4j_url`, `neo4j_password` to the `fmt::Debug` redaction list.

- [ ] **Step 4: Add env var loading to `build_config.rs`**

In the `into_config()` function, add env var resolution (follow the existing pattern — `env::var().ok().unwrap_or_default()` or `env::var().ok().and_then(|v| v.parse().ok()).unwrap_or(default)`):

```rust
neo4j_url: env::var("AXON_NEO4J_URL").ok().unwrap_or_default(),
neo4j_user: env::var("AXON_NEO4J_USER").ok().unwrap_or_else(|| "neo4j".to_string()),
neo4j_password: env::var("AXON_NEO4J_PASSWORD").ok().unwrap_or_default(),
graph_queue: env::var("AXON_GRAPH_QUEUE").ok().unwrap_or_else(|| "axon.graph.jobs".to_string()),
graph_concurrency: env::var("AXON_GRAPH_CONCURRENCY").ok().and_then(|v| v.parse().ok()).unwrap_or(4),
graph_llm_url: env::var("AXON_GRAPH_LLM_URL").ok().unwrap_or_else(|| "http://localhost:11434".to_string()),
graph_llm_model: env::var("AXON_GRAPH_LLM_MODEL").ok().unwrap_or_else(|| "qwen3.5:2b".to_string()),
graph_similarity_threshold: env::var("AXON_GRAPH_SIMILARITY_THRESHOLD").ok().and_then(|v| v.parse().ok()).unwrap_or(0.75),
graph_similarity_limit: env::var("AXON_GRAPH_SIMILARITY_LIMIT").ok().and_then(|v| v.parse().ok()).unwrap_or(20),
graph_context_max_chars: env::var("AXON_GRAPH_CONTEXT_MAX_CHARS").ok().and_then(|v| v.parse().ok()).unwrap_or(2000),
graph_taxonomy_path: env::var("AXON_GRAPH_TAXONOMY_PATH").ok().unwrap_or_default(),
ask_graph: false, // Set from --graph CLI flag
```

Also add CLI parsing for `"graph"` command in the `CliCommand` → `CommandKind` mapping, and `--graph` flag on the ask command.

- [ ] **Step 5: Add `JobTable::Graph` to `common.rs`**

```rust
// In JobTable enum:
Graph,

// In as_str():
Self::Graph => "axon_graph_jobs",
```

- [ ] **Step 6: Update ALL inline `Config` literals in test helpers**

Update `test_config()` in `crates/jobs/common.rs`, `make_test_config()` in `crates/cli/commands/research.rs` and `search.rs`. Since they use `..Config::default()`, the new fields should be covered — but **verify** by running:

```bash
cargo test --lib 2>&1 | grep "error\[" | head -20
```

If any test file has an explicit `Config { field1, field2, ... }` without `..Config::default()`, add the new fields there.

- [ ] **Step 7: Verify compilation**

Run: `cargo check 2>&1 | tail -20`
Expected: 0 errors

Run: `cargo test --lib 2>&1 | grep "test result" | tail -5`
Expected: All existing tests pass

- [ ] **Step 8: Commit**

```bash
git add -A crates/core/config/ crates/jobs/common.rs crates/cli/commands/research.rs crates/cli/commands/search.rs
git commit -m "feat(config): add Neo4j and graph extraction config fields"
```

---

### Task 3: Add `id` Field to Qdrant Types

The graph worker needs Qdrant point IDs to link chunks back to Neo4j entities. Qdrant already returns `id` in scroll/search responses — we just need to capture it.

**Files:**
- Modify: `crates/vector/ops/qdrant/types.rs`

- [ ] **Step 1: Write test for id deserialization**

Add to test module in `types.rs` (create if needed):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qdrant_point_deserializes_with_id() {
        let json = r#"{"id": "550e8400-e29b-41d4-a716-446655440000", "payload": {"url": "https://example.com", "chunk_text": "hello"}}"#;
        let point: QdrantPoint = serde_json::from_str(json).unwrap();
        assert_eq!(point.id, serde_json::json!("550e8400-e29b-41d4-a716-446655440000"));
    }

    #[test]
    fn qdrant_point_deserializes_without_id() {
        let json = r#"{"payload": {"url": "https://example.com"}}"#;
        let point: QdrantPoint = serde_json::from_str(json).unwrap();
        assert!(point.id.is_null());
    }

    #[test]
    fn qdrant_search_hit_deserializes_with_id() {
        let json = r#"{"id": 12345, "score": 0.95, "payload": {"url": "https://example.com"}}"#;
        let hit: QdrantSearchHit = serde_json::from_str(json).unwrap();
        assert_eq!(hit.id, serde_json::json!(12345));
        assert!((hit.score - 0.95).abs() < f64::EPSILON);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test qdrant_point_deserializes -p axon_cli --lib 2>&1 | tail -20`
Expected: FAIL — no field `id`

- [ ] **Step 3: Add `id` field**

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct QdrantPoint {
    #[serde(default)]
    pub id: serde_json::Value,
    #[serde(default)]
    pub payload: QdrantPayload,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QdrantSearchHit {
    #[serde(default)]
    pub id: serde_json::Value,
    pub score: f64,
    #[serde(default)]
    pub payload: QdrantPayload,
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test qdrant_point -p axon_cli --lib 2>&1 | tail -10`
Expected: 3 tests PASS

Also verify no regressions: `cargo test --lib 2>&1 | grep "test result"`

- [ ] **Step 5: Commit**

```bash
git add crates/vector/ops/qdrant/types.rs
git commit -m "feat(qdrant): capture point ID in QdrantPoint and QdrantSearchHit"
```

---

## Chunk 2: Extraction Layers — Taxonomy, Similarity, LLM

### Task 4: Graph Job Module Root + Schema (`crates/jobs/graph.rs`)

The job table for tracking graph extraction status per URL.

**Files:**
- Create: `crates/jobs/graph.rs`
- Modify: parent module declarations to register `pub mod graph;`

**Reference:** Follow `crates/jobs/embed.rs` for the schema pattern. Use `begin_schema_migration_tx()` from `common.rs`.

- [ ] **Step 1: Write test for `ensure_schema` SQL generation**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_job_table_name() {
        assert_eq!(crate::crates::jobs::common::JobTable::Graph.as_str(), "axon_graph_jobs");
    }
}
```

- [ ] **Step 2: Implement the module**

```rust
//! Graph extraction job persistence — tracks which URLs have been processed
//! for entity/relationship extraction into Neo4j.

mod schema;
pub(crate) mod taxonomy;
pub(crate) mod similarity;
pub(crate) mod extract;
pub(crate) mod context;
pub(crate) mod worker;

pub use schema::ensure_graph_schema;
pub use worker::run_graph_worker;
```

- [ ] **Step 3: Create `crates/jobs/graph/schema.rs`**

```rust
//! Neo4j schema setup (constraints, indexes) + Postgres graph job table.

use crate::crates::jobs::common::begin_schema_migration_tx;
use sqlx::PgPool;

/// Ensure the `axon_graph_jobs` table and Neo4j constraints exist.
pub async fn ensure_graph_schema(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = begin_schema_migration_tx(pool, 0xA804_0006).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS axon_graph_jobs (
            id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            url           TEXT NOT NULL,
            status        TEXT NOT NULL DEFAULT 'pending',
            chunk_count   INTEGER DEFAULT 0,
            entity_count  INTEGER DEFAULT 0,
            relation_count INTEGER DEFAULT 0,
            config_json   JSONB,
            error_text    TEXT,
            created_at    TIMESTAMPTZ DEFAULT now(),
            updated_at    TIMESTAMPTZ DEFAULT now(),
            started_at    TIMESTAMPTZ,
            finished_at   TIMESTAMPTZ
        )
        "#,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_graph_jobs_status ON axon_graph_jobs(status)")
        .execute(&mut *tx)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_graph_jobs_url ON axon_graph_jobs(url)")
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

/// Ensure Neo4j constraints and indexes exist.
pub async fn ensure_neo4j_schema(
    neo4j: &crate::crates::core::neo4j::Neo4jClient,
) -> Result<(), Box<dyn std::error::Error>> {
    let constraints = [
        "CREATE CONSTRAINT entity_name IF NOT EXISTS FOR (e:Entity) REQUIRE e.name IS UNIQUE",
        "CREATE CONSTRAINT document_url IF NOT EXISTS FOR (d:Document) REQUIRE d.url IS UNIQUE",
        "CREATE INDEX chunk_point_id IF NOT EXISTS FOR (c:Chunk) ON (c.point_id)",
        "CREATE INDEX entity_type IF NOT EXISTS FOR (e:Entity) ON (e.entity_type)",
    ];
    for cypher in constraints {
        neo4j.execute(cypher, serde_json::json!({})).await?;
    }
    Ok(())
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check 2>&1 | tail -10`

- [ ] **Step 5: Commit**

```bash
git add crates/jobs/graph.rs crates/jobs/graph/schema.rs
git commit -m "feat(jobs): add graph job table schema and Neo4j constraint setup"
```

---

### Task 5: Layer 1 — Taxonomy NER (`crates/jobs/graph/taxonomy.rs`)

Regex + curated taxonomy for fast entity extraction. Zero LLM cost, ~1000 docs/sec. This module is shared — used by both graph construction (Layer 1) and `ask --graph` (entity extraction from top-K results).

**Files:**
- Create: `crates/jobs/graph/taxonomy.rs`
- Reference: `crates/jobs/graph/taxonomy.json` (already exists — 27KB curated taxonomy)

**Key design decisions:**
- `Taxonomy` struct loaded once via `include_str!()` for the built-in taxonomy, or from `AXON_GRAPH_TAXONOMY_PATH` for custom overrides
- `extract_entities(text, source_type) -> Vec<EntityCandidate>` is the single public API
- Five regex extractors: imports, file paths, CLI commands, URLs, taxonomy lookup
- Entities are deduplicated by normalized name within a chunk
- Ambiguous entities (taxonomy `"ambiguous": true` or regex-only matches) flagged for Layer 3

- [ ] **Step 0: Verify/seed `taxonomy.json`**

The file `crates/jobs/graph/taxonomy.json` already exists (27KB). Verify it follows the spec's schema and has ≥248 entries. If it needs seeding or updating, the entries should include at minimum:

```json
{
  "version": "1.0",
  "entries": [
    {"name": "Tokio", "type": "technology", "aliases": ["tokio-rs"], "category": "async-runtime"},
    {"name": "PostgreSQL", "type": "service", "aliases": ["postgres", "pg", "psql"], "category": "database"},
    {"name": "Docker", "type": "technology", "aliases": ["docker-compose", "dockerfile"], "category": "container"},
    {"name": "React", "type": "technology", "aliases": ["reactjs", "react.js"], "category": "ui-framework", "ambiguous": true, "disambiguation_hint": "library/framework context vs verb"},
    {"name": "Neo4j", "type": "service", "aliases": ["neo4j-browser", "cypher"], "category": "graph-database"}
  ]
}
```

Verify: `python3 -c "import json; d=json.load(open('crates/jobs/graph/taxonomy.json')); print(len(d.get('entries', [])))"`
Expected: ≥248 entries

- [ ] **Step 1: Write tests for taxonomy loading + entity extraction**

Write tests for:
1. Taxonomy JSON deserialization
2. Import statement extraction (Rust `use`, Python `import/from`, JS `import/require`)
3. Taxonomy lookup (case-insensitive, alias resolution)
4. Ambiguity detection
5. Deduplication within a chunk
6. Source-type filtering (github → import patterns strongest)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taxonomy_loads_from_embedded_json() {
        let tax = Taxonomy::builtin();
        assert!(tax.entries.len() > 100, "Expected >100 entries, got {}", tax.entries.len());
    }

    #[test]
    fn extract_rust_use_statement() {
        let tax = Taxonomy::builtin();
        let candidates = tax.extract_entities("use tokio::sync::Mutex;", "github");
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"Tokio"), "Expected Tokio in {:?}", names);
    }

    #[test]
    fn extract_python_import() {
        let tax = Taxonomy::builtin();
        let candidates = tax.extract_entities("from fastapi import FastAPI", "crawl");
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"FastAPI") || names.iter().any(|n| n.eq_ignore_ascii_case("fastapi")));
    }

    #[test]
    fn extract_js_import() {
        let tax = Taxonomy::builtin();
        let candidates = tax.extract_entities("import React from 'react';", "crawl");
        assert!(candidates.iter().any(|c| c.name.eq_ignore_ascii_case("react")));
    }

    #[test]
    fn taxonomy_lookup_case_insensitive() {
        let tax = Taxonomy::builtin();
        let candidates = tax.extract_entities("We use DOCKER for containerization", "crawl");
        assert!(candidates.iter().any(|c| c.name == "Docker"));
    }

    #[test]
    fn taxonomy_alias_resolution() {
        let tax = Taxonomy::builtin();
        let candidates = tax.extract_entities("Connect to postgres database", "crawl");
        assert!(candidates.iter().any(|c| c.name == "PostgreSQL"));
    }

    #[test]
    fn deduplication_within_chunk() {
        let tax = Taxonomy::builtin();
        let text = "use tokio::sync; use tokio::time; use tokio::net;";
        let candidates = tax.extract_entities(text, "github");
        let tokio_count = candidates.iter().filter(|c| c.name == "Tokio").count();
        assert_eq!(tokio_count, 1, "Tokio should appear once, got {}", tokio_count);
    }

    #[test]
    fn ambiguous_entity_flagged() {
        let tax = Taxonomy::builtin();
        // "React" is marked ambiguous in taxonomy (common English word)
        let candidates = tax.extract_entities("import React from 'react';", "crawl");
        let react = candidates.iter().find(|c| c.name.eq_ignore_ascii_case("react"));
        // When found via import pattern, confidence is high regardless of ambiguity flag
        assert!(react.is_some());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test taxonomy -p axon_cli --lib 2>&1 | tail -20`
Expected: FAIL — module not implemented

- [ ] **Step 3: Implement `Taxonomy` struct and extraction logic**

The implementation should:
1. Define `TaxonomyEntry` and `EntityCandidate` structs
2. Load taxonomy from `include_str!("taxonomy.json")` or custom path
3. Build a `HashMap<String, &TaxonomyEntry>` keyed by lowercase name + all aliases for O(1) lookup
4. Implement 5 regex extractors (import, file path, CLI command, URL, taxonomy word-boundary scan)
5. Normalize entity names (lowercase for lookup, canonical name from taxonomy for output)
6. Deduplicate by normalized name
7. Flag ambiguous candidates

Key structs:

```rust
#[derive(Debug, Clone)]
pub struct EntityCandidate {
    pub name: String,
    pub entity_type: String,
    pub confidence: f32,
    pub source: CandidateSource,
    pub ambiguous: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum CandidateSource {
    Import,
    FilePath,
    CliCommand,
    Url,
    Taxonomy,
}

pub struct Taxonomy {
    entries: Vec<TaxonomyEntry>,
    lookup: HashMap<String, usize>,  // lowercase name/alias → entries index
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test taxonomy -p axon_cli --lib 2>&1 | tail -20`
Expected: 8+ tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/jobs/graph/taxonomy.rs
git commit -m "feat(graph): Layer 1 taxonomy NER with regex extractors and curated tech taxonomy"
```

---

### Task 6: Layer 2 — Embedding Similarity (`crates/jobs/graph/similarity.rs`)

Uses Qdrant's recommend API to find semantically similar documents using existing vectors. Creates `SIMILAR_TO` edges in Neo4j. Zero LLM cost.

**Files:**
- Create: `crates/jobs/graph/similarity.rs`

**Key design decisions:**
- Deterministic point ID: `Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("{url}:{chunk_index}").as_bytes())`
- Use Qdrant `/points/query` with `recommend.positive` strategy
- Batch via `/collections/{col}/query/batch` for bulk processing
- Score threshold from `cfg.graph_similarity_threshold` (default 0.75)
- `MERGE` in Neo4j prevents duplicate edges

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_point_id_is_deterministic() {
        let id1 = chunk_point_id("https://tokio.rs/tutorial", 0);
        let id2 = chunk_point_id("https://tokio.rs/tutorial", 0);
        assert_eq!(id1, id2);
    }

    #[test]
    fn chunk_point_id_varies_by_url() {
        let id1 = chunk_point_id("https://tokio.rs/tutorial", 0);
        let id2 = chunk_point_id("https://axum.rs/tutorial", 0);
        assert_ne!(id1, id2);
    }

    #[test]
    fn chunk_point_id_varies_by_index() {
        let id1 = chunk_point_id("https://tokio.rs/tutorial", 0);
        let id2 = chunk_point_id("https://tokio.rs/tutorial", 1);
        assert_ne!(id1, id2);
    }

    #[test]
    fn build_recommend_request_structure() {
        let req = build_recommend_request("cortex", "https://example.com", 0.75, 20);
        assert!(req["query"]["recommend"]["positive"].as_array().unwrap().len() == 1);
        assert!(req["filter"]["must_not"].as_array().unwrap().len() == 1);
    }

    #[test]
    fn similarity_edge_construction() {
        let edge = SimilarityEdge {
            source_url: "https://a.com".to_string(),
            target_url: "https://b.com".to_string(),
            score: 0.87,
            target_source_type: "crawl".to_string(),
        };
        assert!(edge.score > 0.75);
    }

    #[test]
    fn group_results_by_url_takes_max_score() {
        let results = vec![
            ("https://b.com".to_string(), 0.82, "crawl".to_string()),
            ("https://b.com".to_string(), 0.91, "crawl".to_string()),
            ("https://c.com".to_string(), 0.78, "github".to_string()),
        ];
        let grouped = group_by_url_max_score(results);
        assert_eq!(grouped.len(), 2);
        let b = grouped.iter().find(|e| e.target_url == "https://b.com").unwrap();
        assert!((b.score - 0.91).abs() < f64::EPSILON as f32);
    }
}
```

- [ ] **Step 2: Run tests, verify fail**

- [ ] **Step 3: Implement**

Key functions:
- `pub fn chunk_point_id(url: &str, chunk_index: usize) -> uuid::Uuid`
- `pub fn build_recommend_request(collection: &str, url: &str, threshold: f64, limit: usize) -> Value`
- `pub fn group_by_url_max_score(results: Vec<(String, f32, String)>) -> Vec<SimilarityEdge>`
- `pub async fn compute_similarity(cfg: &Config, neo4j: &Neo4jClient, url: &str) -> Result<Vec<SimilarityEdge>>`
- Neo4j write: `MERGE (d1:Document {url: $source_url}) MERGE (d2:Document {url: $target_url}) MERGE (d1)-[r:SIMILAR_TO]->(d2) SET r.score = $score, r.updated_at = datetime()`

- [ ] **Step 4: Run tests, verify pass**

- [ ] **Step 5: Commit**

```bash
git add crates/jobs/graph/similarity.rs
git commit -m "feat(graph): Layer 2 embedding similarity via Qdrant recommend API"
```

---

### Task 7: Layer 3 — LLM Extraction (`crates/jobs/graph/extract.rs`)

Ollama-based entity/relationship extraction for ambiguous entities. Uses Qwen 3.5 2B with JSON schema enforcement via `/api/generate`.

**Files:**
- Create: `crates/jobs/graph/extract.rs`

**Critical parameters:**
- Endpoint: `{graph_llm_url}/api/generate` (NOT `/api/chat`)
- `"think": false` — required for Qwen 3.5 to produce content
- `"format": {...}` — JSON schema with enum constraints for entity types and relation types
- `"stream": false` — wait for complete response

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_extraction_request_has_required_fields() {
        let req = build_extraction_request("qwen3.5:2b", "Some document text about Rust");
        assert_eq!(req["model"], "qwen3.5:2b");
        assert_eq!(req["think"], false);
        assert_eq!(req["stream"], false);
        assert!(req["format"]["properties"]["entities"].is_object());
        assert!(req["format"]["properties"]["relationships"].is_object());
    }

    #[test]
    fn parse_extraction_response_valid() {
        let response = serde_json::json!({
            "entities": [
                {"name": "Tokio", "type": "technology"},
                {"name": "Axon", "type": "project"}
            ],
            "relationships": [
                {"source": "Axon", "target": "Tokio", "relation": "USES"}
            ]
        });
        let result = parse_extraction_response(&response.to_string()).unwrap();
        assert_eq!(result.entities.len(), 2);
        assert_eq!(result.relationships.len(), 1);
    }

    #[test]
    fn parse_extraction_response_empty_entities() {
        let response = r#"{"entities": [], "relationships": []}"#;
        let result = parse_extraction_response(response).unwrap();
        assert!(result.entities.is_empty());
    }

    #[test]
    fn normalize_entity_name_strips_suffixes() {
        assert_eq!(normalize_entity_name("tokio::new()"), "tokio");
        assert_eq!(normalize_entity_name("config.rs"), "config");
        assert_eq!(normalize_entity_name("PostgreSQL"), "postgresql");
    }

    #[test]
    fn resolve_type_conflict_most_specific_wins() {
        assert_eq!(resolve_type_conflict("technology", "concept"), "technology");
        assert_eq!(resolve_type_conflict("concept", "service"), "service");
    }
}
```

- [ ] **Step 2: Run tests, verify fail**

- [ ] **Step 3: Implement**

Key functions:
- `pub fn build_extraction_request(model: &str, text: &str) -> Value`
- `pub fn parse_extraction_response(json_str: &str) -> Result<ExtractionResult>`
- `pub fn normalize_entity_name(name: &str) -> String`
- `pub fn resolve_type_conflict(type_a: &str, type_b: &str) -> String`
- `pub async fn extract_entities_llm(cfg: &Config, text: &str) -> Result<ExtractionResult>`

Structs:
```rust
pub struct ExtractionResult {
    pub entities: Vec<ExtractedEntity>,
    pub relationships: Vec<ExtractedRelationship>,
}

pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: String,
}

pub struct ExtractedRelationship {
    pub source: String,
    pub target: String,
    pub relation: String,
}
```

- [ ] **Step 4: Run tests, verify pass**

- [ ] **Step 5: Commit**

```bash
git add crates/jobs/graph/extract.rs
git commit -m "feat(graph): Layer 3 LLM entity extraction via Ollama"
```

---

## Chunk 3: Graph Context for Ask + Worker

### Task 8: GraphContext Builder (`crates/jobs/graph/context.rs`)

Builds the structured graph context that gets prepended to `ask` LLM prompts. Used by `ask --graph`. Reuses the taxonomy from Task 5 to extract entity names from top-K vector results, then queries Neo4j for 1-hop neighborhoods.

**Files:**
- Create: `crates/jobs/graph/context.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_graph_context_empty() {
        let ctx = GraphContext {
            context_text: String::new(),
            entities: vec![],
            neighbor_chunk_ids: vec![],
            similar_docs: vec![],
        };
        assert!(ctx.context_text.is_empty());
    }

    #[test]
    fn format_entity_block() {
        let entity = GraphEntity {
            name: "Tokio".to_string(),
            entity_type: "technology".to_string(),
            description: "async runtime for Rust".to_string(),
            relations: vec![
                GraphRelation {
                    relation: "USED_BY".to_string(),
                    target_name: "axum".to_string(),
                    target_type: "technology".to_string(),
                },
            ],
            doc_count: 12,
            chunk_count: 47,
        };
        let text = format_entity(&entity);
        assert!(text.contains("Tokio"));
        assert!(text.contains("technology"));
        assert!(text.contains("USED_BY"));
        assert!(text.contains("axum"));
    }

    #[test]
    fn format_context_respects_char_budget() {
        let entities: Vec<GraphEntity> = (0..50)
            .map(|i| GraphEntity {
                name: format!("Entity{i}"),
                entity_type: "technology".to_string(),
                description: "A technology that does stuff and has a long description".to_string(),
                relations: vec![],
                doc_count: 1,
                chunk_count: 1,
            })
            .collect();
        let text = format_context_text(&entities, 500);
        assert!(text.len() <= 600, "Should be near budget, got {}", text.len());
        // Should truncate at entity boundary
        assert!(!text.ends_with("Ent"));
    }

    #[test]
    fn entities_prioritized_by_relation_count() {
        let mut entities = vec![
            GraphEntity {
                name: "A".to_string(),
                entity_type: "technology".to_string(),
                description: String::new(),
                relations: vec![],  // 0 relations
                doc_count: 100,
                chunk_count: 100,
            },
            GraphEntity {
                name: "B".to_string(),
                entity_type: "technology".to_string(),
                description: String::new(),
                relations: vec![
                    GraphRelation { relation: "USES".to_string(), target_name: "C".to_string(), target_type: "technology".to_string() },
                    GraphRelation { relation: "USES".to_string(), target_name: "D".to_string(), target_type: "technology".to_string() },
                ],
                doc_count: 1,
                chunk_count: 1,
            },
        ];
        sort_entities_by_priority(&mut entities);
        assert_eq!(entities[0].name, "B");  // More relations = higher priority
    }
}
```

- [ ] **Step 2: Run tests, verify fail**

- [ ] **Step 3: Implement**

Key structs and functions:

```rust
pub struct GraphContext {
    pub context_text: String,
    pub entities: Vec<GraphEntity>,
    pub neighbor_chunk_ids: Vec<String>,
    pub similar_docs: Vec<super::similarity::SimilarityEdge>,
}

pub struct GraphEntity {
    pub name: String,
    pub entity_type: String,
    pub description: String,
    pub relations: Vec<GraphRelation>,
    pub doc_count: u32,
    pub chunk_count: u32,
}

pub struct GraphRelation {
    pub relation: String,
    pub target_name: String,
    pub target_type: String,
}

pub fn format_entity(entity: &GraphEntity) -> String { ... }
pub fn format_context_text(entities: &[GraphEntity], max_chars: usize) -> String { ... }
pub fn sort_entities_by_priority(entities: &mut [GraphEntity]) { ... }

/// Build graph context for ask --graph.
/// 1. Extract entity names from top-K chunk texts via taxonomy
/// 2. Neo4j 1-hop traversal from matched entities
/// 3. Format as structured text block, capped at max_chars
pub async fn build_graph_context(
    cfg: &Config,
    neo4j: &Neo4jClient,
    chunk_texts: &[String],
) -> Result<GraphContext, Box<dyn std::error::Error>> { ... }
```

The Neo4j query for step 2:
```cypher
MATCH (e:Entity) WHERE e.name IN $entity_names
WITH e
OPTIONAL MATCH (e)-[r:RELATES_TO]-(neighbor:Entity)
WITH e, collect({name: neighbor.name, type: neighbor.entity_type, relation: r.relation}) AS neighbors
OPTIONAL MATCH (e)-[:MENTIONED_IN]->(c:Chunk)-[:BELONGS_TO]->(d:Document)
WITH e, neighbors, count(DISTINCT d) AS doc_count, count(c) AS chunk_count
RETURN e.name AS name, e.entity_type AS type, e.description AS description,
       neighbors, doc_count, chunk_count
ORDER BY size(neighbors) DESC
```

- [ ] **Step 4: Run tests, verify pass**

- [ ] **Step 5: Commit**

```bash
git add crates/jobs/graph/context.rs
git commit -m "feat(graph): GraphContext builder for ask --graph enrichment"
```

---

### Task 9: Integrate Graph Context into Ask Pipeline

Wire `build_graph_context()` into the existing `ask` pipeline when `--graph` is set.

**Files:**
- Modify: `crates/vector/ops/commands/ask.rs` — call graph context when `cfg.ask_graph`
- Modify: `crates/vector/ops/commands/ask/context.rs` — add `graph_context` field to `AskContext`

**Key design:**
- Only runs when `cfg.ask_graph == true` AND `cfg.neo4j_url` is non-empty
- If Neo4j is unreachable, fall back to vector-only with a warning (don't fail the ask)
- Graph context is prepended to the existing context string
- New timing field: `graph_ms` in diagnostics

- [ ] **Step 1: Add `graph_context` field to `AskContext`**

In `crates/vector/ops/commands/ask/context.rs`:

```rust
pub(crate) struct AskContext {
    pub context: String,
    pub graph_context_text: String,  // NEW — empty when --graph not used
    pub graph_entities_found: usize, // NEW
    // ... existing fields unchanged
}
```

- [ ] **Step 2: Modify `build_ask_context` to optionally enrich with graph**

After `build_context_from_candidates()`, if `cfg.ask_graph` and Neo4j is available:
1. Extract chunk texts from the candidates
2. Call `build_graph_context(cfg, neo4j, &chunk_texts)`
3. Prepend `graph_context.context_text` to the assembled context
4. Set `graph_context_text` and `graph_entities_found` on AskContext

- [ ] **Step 3: Update `ask_payload()` in `ask.rs` to include graph diagnostics**

Add to the diagnostics JSON:
```rust
"graph_entities": ctx.graph_entities_found,
"graph_context_chars": ctx.graph_context_text.len(),
```

- [ ] **Step 4: Verify compilation and existing tests pass**

Run: `cargo check && cargo test ask -p axon_cli --lib 2>&1 | tail -20`
Expected: All existing ask tests still pass (graph context is empty by default)

- [ ] **Step 5: Commit**

```bash
git add crates/vector/ops/commands/ask.rs crates/vector/ops/commands/ask/context.rs
git commit -m "feat(ask): integrate graph context enrichment when --graph is set"
```

---

### Task 10: Graph Worker (`crates/jobs/graph/worker.rs`)

AMQP worker that consumes graph extraction jobs and orchestrates all 3 layers.

**Files:**
- Create: `crates/jobs/graph/worker.rs`

**Reference:** Follow `crates/jobs/worker_lane.rs` pattern (used by embed/extract/refresh).

**Worker loop:**
1. Claim pending graph job from Postgres
2. Retrieve all chunks for the URL from Qdrant
3. Layer 1: taxonomy NER on each chunk
4. Layer 3: LLM extraction for ambiguous entities (conditional)
5. Layer 2: embedding similarity (independent of L1/L3)
6. Write entities + relationships to Neo4j via MERGE
7. Mark job completed with counts

- [ ] **Step 0: Write test for graph job processing logic**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_entity_candidates_deduplicates() {
        let taxonomy_candidates = vec![
            EntityCandidate { name: "Tokio".to_string(), entity_type: "technology".to_string(), confidence: 0.95, source: CandidateSource::Taxonomy, ambiguous: false },
            EntityCandidate { name: "Tokio".to_string(), entity_type: "technology".to_string(), confidence: 0.9, source: CandidateSource::Import, ambiguous: false },
        ];
        let merged = merge_candidates(taxonomy_candidates);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name, "Tokio");
    }

    #[test]
    fn separate_ambiguous_candidates() {
        let candidates = vec![
            EntityCandidate { name: "Docker".to_string(), entity_type: "technology".to_string(), confidence: 0.95, source: CandidateSource::Taxonomy, ambiguous: false },
            EntityCandidate { name: "React".to_string(), entity_type: "technology".to_string(), confidence: 0.8, source: CandidateSource::Taxonomy, ambiguous: true },
        ];
        let (clear, ambiguous) = partition_by_ambiguity(candidates);
        assert_eq!(clear.len(), 1);
        assert_eq!(ambiguous.len(), 1);
        assert_eq!(ambiguous[0].name, "React");
    }
}
```

- [ ] **Step 1: Implement worker loop**

Follow the `worker_lane.rs` pattern used by embed/extract workers. The worker loop is:
1. Create Neo4j client + PgPool + AMQP channel
2. Ensure Postgres schema + Neo4j constraints
3. Declare AMQP queue (durable)
4. Set prefetch (cfg.graph_concurrency)
5. Enter consume loop: for each delivery → claim job → process → ack/nack
6. On AMQP disconnect → exponential backoff reconnect (2s initial, 60s cap)

```rust
use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_info, log_warn};
use crate::crates::core::neo4j::Neo4jClient;
use crate::crates::jobs::common::{make_pool, open_amqp_channel, claim_pending_by_id, mark_job_completed, mark_job_failed, JobTable};
use super::schema::{ensure_graph_schema, ensure_neo4j_schema};

pub async fn run_graph_worker(cfg: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let neo4j = Neo4jClient::from_config(cfg)
        .ok_or("AXON_NEO4J_URL is required for graph worker")?;

    let pool = make_pool(cfg).await?;
    ensure_graph_schema(&pool).await?;
    ensure_neo4j_schema(&neo4j).await?;

    log_info("graph worker started — entering AMQP consume loop");

    let mut backoff_secs = 2u64;
    loop {
        match run_graph_consumer(cfg, &neo4j, &pool).await {
            Ok(()) => {
                log_info("graph consumer exited cleanly, restarting");
                backoff_secs = 2;
            }
            Err(e) => {
                log_warn(&format!("graph consumer error: {e} — reconnecting in {backoff_secs}s"));
                tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(60);
            }
        }
    }
}

async fn run_graph_consumer(
    cfg: &Config,
    neo4j: &Neo4jClient,
    pool: &sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let (_, channel) = open_amqp_channel(&cfg.amqp_url).await?;

    // Declare durable queue
    channel.queue_declare(
        &cfg.graph_queue,
        lapin::options::QueueDeclareOptions { durable: true, ..Default::default() },
        lapin::types::FieldTable::default(),
    ).await?;

    channel.basic_qos(cfg.graph_concurrency as u16, lapin::options::BasicQosOptions::default()).await?;

    let mut consumer = channel.basic_consume(
        &cfg.graph_queue,
        "graph-worker",
        lapin::options::BasicConsumeOptions::default(),
        lapin::types::FieldTable::default(),
    ).await?;

    use futures_lite::StreamExt;
    while let Some(delivery) = consumer.next().await {
        let delivery = delivery?;
        let job_id: uuid::Uuid = serde_json::from_slice(&delivery.data)?;

        let job = claim_pending_by_id(pool, job_id, JobTable::Graph).await?;
        if job.is_none() {
            delivery.ack(lapin::options::BasicAckOptions::default()).await?;
            continue;
        }

        match process_graph_job(cfg, neo4j, pool, &job.unwrap()).await {
            Ok((entity_count, relation_count)) => {
                let result = serde_json::json!({
                    "entity_count": entity_count,
                    "relation_count": relation_count,
                });
                mark_job_completed(pool, job_id, JobTable::Graph, Some(result)).await?;
            }
            Err(e) => {
                mark_job_failed(pool, job_id, JobTable::Graph, &e.to_string()).await?;
            }
        }

        delivery.ack(lapin::options::BasicAckOptions::default()).await?;
    }

    Ok(())
}
```

- [ ] **Step 2: Implement `process_graph_job`**

```rust
async fn process_graph_job(
    cfg: &Config,
    neo4j: &Neo4jClient,
    pool: &PgPool,
    url: &str,
) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    // 1. Retrieve chunks from Qdrant for this URL
    // 2. Run taxonomy on each chunk → collect EntityCandidates
    // 3. For ambiguous candidates → LLM extraction
    // 4. Write entities to Neo4j (MERGE)
    // 5. Write relationships to Neo4j (MERGE)
    // 6. Write MENTIONED_IN edges (entity → chunk)
    // 7. Run similarity for this URL → write SIMILAR_TO edges
    // 8. Return (entity_count, relation_count)
}
```

- [ ] **Step 3: Wire Neo4j writes**

Cypher patterns:
```cypher
-- Entity MERGE
MERGE (e:Entity {name: $name})
SET e.entity_type = $type, e.description = $description, e.updated_at = datetime()

-- Relationship MERGE
MATCH (s:Entity {name: $source}), (t:Entity {name: $target})
MERGE (s)-[r:RELATES_TO]->(t)
SET r.relation = $relation

-- Chunk node + MENTIONED_IN
MERGE (c:Chunk {point_id: $point_id})
SET c.url = $url, c.collection = $collection, c.chunk_index = $chunk_index
MERGE (e:Entity {name: $entity_name})-[:MENTIONED_IN]->(c)

-- Document node + BELONGS_TO
MERGE (d:Document {url: $url})
SET d.domain = $domain, d.source_type = $source_type, d.extracted_at = datetime()
MERGE (c:Chunk {point_id: $point_id})-[:BELONGS_TO]->(d)
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check 2>&1 | tail -10`

- [ ] **Step 5: Commit**

```bash
git add crates/jobs/graph/worker.rs
git commit -m "feat(graph): AMQP graph worker with 3-layer extraction pipeline"
```

---

## Chunk 4: CLI Commands + Services + Wiring

### Task 11: Graph Service Layer (`crates/services/graph.rs`)

Typed service functions called by both CLI and MCP handlers.

**Files:**
- Create: `crates/services/graph.rs`
- Modify: `crates/services/types/service.rs` — add result types
- Modify: `crates/services/mod.rs` (or wherever services are declared) — register module

- [ ] **Step 1: Add service result types**

In `crates/services/types/service.rs`, add:

```rust
// ── Graph ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct GraphBuildResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphStatusResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphExploreResult {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphStatsResult {
    pub payload: serde_json::Value,
}
```

- [ ] **Step 2: Implement service functions**

```rust
pub async fn graph_build(cfg: &Config, url: Option<&str>, domain: Option<&str>, all: bool) -> Result<GraphBuildResult, Box<dyn Error>>
pub async fn graph_status(cfg: &Config) -> Result<GraphStatusResult, Box<dyn Error>>
pub async fn graph_explore(cfg: &Config, entity: &str) -> Result<GraphExploreResult, Box<dyn Error>>
pub async fn graph_stats(cfg: &Config) -> Result<GraphStatsResult, Box<dyn Error>>
```

Each creates a `Neo4jClient` from config, returns typed result. `graph_build` with a single URL runs synchronously; with `--all` it enqueues jobs.

- [ ] **Step 3: Commit**

```bash
git add crates/services/graph.rs crates/services/types/service.rs
git commit -m "feat(services): add graph service layer with typed results"
```

---

### Task 12: Graph CLI Command (`crates/cli/commands/graph.rs`)

CLI handler with subcommands: `build`, `status`, `explore`, `stats`, `worker`.

**Files:**
- Create: `crates/cli/commands/graph.rs`
- Modify: `crates/cli/commands.rs` — add `pub mod graph;` + `pub use graph::run_graph;`
- Modify: `lib.rs` — add `CommandKind::Graph => run_graph(cfg).await?` dispatch arm

**Pattern:** Follow `crates/cli/commands/crawl.rs` for subcommand routing.

- [ ] **Step 1: Implement `run_graph` with subcommand routing**

```rust
pub async fn run_graph(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let sub = cfg.positional.first().map(|s| s.as_str());
    match sub {
        Some("build") => handle_build(cfg).await,
        Some("status") => handle_status(cfg).await,
        Some("explore") => handle_explore(cfg).await,
        Some("stats") => handle_stats(cfg).await,
        Some("worker") => handle_worker(cfg).await,
        _ => {
            eprintln!("Usage: axon graph <build|status|explore|stats|worker>");
            Ok(())
        }
    }
}
```

- [ ] **Step 2: Implement `handle_build`**

Parse `--url`, `--domain`, `--all` from positional args (index 1+):

```rust
async fn handle_build(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let mut url: Option<String> = None;
    let mut domain: Option<String> = None;
    let mut all = false;

    let mut i = 1;
    while i < cfg.positional.len() {
        match cfg.positional[i].as_str() {
            "--url" => { url = cfg.positional.get(i + 1).cloned(); i += 2; }
            "--domain" => { domain = cfg.positional.get(i + 1).cloned(); i += 2; }
            "--all" => { all = true; i += 1; }
            other => { url = Some(other.to_string()); i += 1; }
        }
    }

    let result = graph_svc::graph_build(cfg, url.as_deref(), domain.as_deref(), all).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
    } else {
        let entities = result.payload["entity_count"].as_u64().unwrap_or(0);
        let relations = result.payload["relation_count"].as_u64().unwrap_or(0);
        println!("{} Extracted {} entities, {} relationships",
            primary("Graph build:"), entities, relations);
    }
    Ok(())
}
```

- [ ] **Step 3: Implement `handle_status`**

```rust
async fn handle_status(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let result = graph_svc::graph_status(cfg).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
        return Ok(());
    }

    let p = &result.payload;
    println!("{}", primary("Graph Extraction Status:"));
    println!("  Total indexed URLs:     {}", p["total_urls"].as_u64().unwrap_or(0));
    println!("  Extracted:              {} ({}%)",
        p["extracted"].as_u64().unwrap_or(0),
        p["extracted_pct"].as_f64().unwrap_or(0.0));
    println!("  Pending:                {}", p["pending"].as_u64().unwrap_or(0));
    println!("  Failed:                 {}", p["failed"].as_u64().unwrap_or(0));
    println!();
    println!("  {}", primary("Neo4j:"));
    println!("    Entities:             {}", p["neo4j_entities"].as_u64().unwrap_or(0));
    println!("    Relationships:        {}", p["neo4j_relationships"].as_u64().unwrap_or(0));
    println!("    Documents:            {}", p["neo4j_documents"].as_u64().unwrap_or(0));
    Ok(())
}
```

- [ ] **Step 4: Implement `handle_explore`**

Display entity neighborhood per the spec's output format:

```rust
async fn handle_explore(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let entity = cfg.positional.get(1).ok_or("Usage: axon graph explore <entity>")?;
    let result = graph_svc::graph_explore(cfg, entity).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
        return Ok(());
    }

    let p = &result.payload;
    // Header: entity name + type
    println!("{} {} ({})",
        primary("█"),
        primary(p["name"].as_str().unwrap_or(entity)),
        muted(p["type"].as_str().unwrap_or("unknown")));
    if let Some(desc) = p["description"].as_str() {
        println!("  {}", desc);
    }
    println!();

    // Relationships grouped by type
    if let Some(rels) = p["relationships"].as_object() {
        let total: usize = rels.values().filter_map(|v| v.as_array()).map(|a| a.len()).sum();
        println!("  {} ({}):", primary("Relationships"), total);
        for (rel_type, targets) in rels {
            let names: Vec<&str> = targets.as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            println!("    {} ({}):  {}", accent(rel_type), names.len(), names.join(", "));
        }
    }
    println!();

    // Mention counts
    if let Some(mentions) = p["mentions"].as_array() {
        let total_chunks: u64 = mentions.iter().filter_map(|m| m["chunks"].as_u64()).sum();
        println!("  Mentioned in {} chunks across {} documents:",
            total_chunks, mentions.len());
        for m in mentions.iter().take(5) {
            println!("    {}  ({} chunks)",
                m["url"].as_str().unwrap_or("?"),
                m["chunks"].as_u64().unwrap_or(0));
        }
        if mentions.len() > 5 { println!("    ..."); }
    }
    println!();

    // Similar documents
    if let Some(similar) = p["similar_docs"].as_array() {
        if !similar.is_empty() {
            println!("  {}:", primary("Similar documents (SIMILAR_TO)"));
            for s in similar.iter().take(10) {
                println!("    {:.2}  {}",
                    s["score"].as_f64().unwrap_or(0.0),
                    s["url"].as_str().unwrap_or("?"));
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Implement `handle_stats`**

```rust
async fn handle_stats(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let result = graph_svc::graph_stats(cfg).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
        return Ok(());
    }

    let p = &result.payload;
    if let Some(types) = p["entity_types"].as_object() {
        let parts: Vec<String> = types.iter()
            .map(|(k, v)| format!("{}({})", k, v.as_u64().unwrap_or(0)))
            .collect();
        println!("{} {}", primary("Entity types:"), parts.join(", "));
    }
    if let Some(rels) = p["relationship_types"].as_object() {
        let parts: Vec<String> = rels.iter()
            .map(|(k, v)| format!("{}({})", k, v.as_u64().unwrap_or(0)))
            .collect();
        println!("{} {}", primary("Relationship types:"), parts.join(", "));
    }
    println!("{} {}  |  {} {}",
        primary("Total entities:"), p["total_entities"].as_u64().unwrap_or(0),
        primary("Total relationships:"), p["total_relationships"].as_u64().unwrap_or(0));

    if let Some(top) = p["most_connected"].as_array() {
        let parts: Vec<String> = top.iter().take(5)
            .map(|e| format!("{} ({})",
                e["name"].as_str().unwrap_or("?"),
                e["count"].as_u64().unwrap_or(0)))
            .collect();
        println!("{} {}", primary("Most connected:"), parts.join(", "));
    }
    Ok(())
}
```

- [ ] **Step 6: Implement `handle_worker`**

```rust
async fn handle_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    crate::crates::jobs::graph::run_graph_worker(cfg).await
}
```

- [ ] **Step 7: Wire dispatch in `lib.rs`**

Add to `run_once()` match:
```rust
CommandKind::Graph => run_graph(cfg).await?,
```

Add to imports in `lib.rs`:
```rust
use self::crates::cli::commands::run_graph;
```

- [ ] **Step 5: Verify `axon graph --help` works**

Run: `cargo run --bin axon -- graph 2>&1 | head -5`
Expected: Usage message

- [ ] **Step 6: Commit**

```bash
git add crates/cli/commands/graph.rs crates/cli/commands.rs lib.rs
git commit -m "feat(cli): add axon graph command with build/status/explore/stats/worker subcommands"
```

---

### Task 13: MCP Graph Action (`crates/mcp/schema.rs` + handler)

Add `graph` action to the MCP tool schema with subactions: `build`, `status`, `explore`, `stats`.

**Files:**
- Modify: `crates/mcp/schema.rs` — add `Graph(GraphRequest)` to `AxonRequest`
- Create: `crates/mcp/server/handlers_graph.rs` — handler implementation
- Modify: `crates/mcp/server.rs` — add dispatch arm for Graph

- [ ] **Step 1: Add schema types**

In `crates/mcp/schema.rs`:

```rust
// Add variant to AxonRequest enum:
Graph(GraphRequest),

// New types:
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphSubaction {
    Build,
    Status,
    Explore,
    Stats,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphRequest {
    pub subaction: GraphSubaction,
    pub url: Option<String>,
    pub domain: Option<String>,
    pub entity: Option<String>,
    pub all: Option<bool>,
    pub response_mode: Option<ResponseMode>,
}
```

- [ ] **Step 2: Create handler**

In `crates/mcp/server/handlers_graph.rs`, follow the pattern of existing MCP handlers (e.g., `handlers_query.rs`):

```rust
use crate::crates::core::config::Config;
use crate::crates::mcp::schema::{GraphRequest, GraphSubaction};
use crate::crates::services::graph as graph_svc;

pub(crate) async fn handle_graph(
    cfg: &Config,
    req: GraphRequest,
) -> Result<serde_json::Value, String> {
    match req.subaction {
        GraphSubaction::Build => {
            let url = req.url.as_deref();
            let domain = req.domain.as_deref();
            let all = req.all.unwrap_or(false);
            if url.is_none() && domain.is_none() && !all {
                return Err("graph build requires one of: url, domain, or all=true".to_string());
            }
            let result = graph_svc::graph_build(cfg, url, domain, all)
                .await
                .map_err(|e| e.to_string())?;
            Ok(result.payload)
        }
        GraphSubaction::Status => {
            let result = graph_svc::graph_status(cfg)
                .await
                .map_err(|e| e.to_string())?;
            Ok(result.payload)
        }
        GraphSubaction::Explore => {
            let entity = req.entity.as_deref()
                .ok_or_else(|| "graph explore requires 'entity' field".to_string())?;
            let result = graph_svc::graph_explore(cfg, entity)
                .await
                .map_err(|e| e.to_string())?;
            Ok(result.payload)
        }
        GraphSubaction::Stats => {
            let result = graph_svc::graph_stats(cfg)
                .await
                .map_err(|e| e.to_string())?;
            Ok(result.payload)
        }
    }
}
```

- [ ] **Step 3: Wire dispatch**

In `crates/mcp/server.rs`, add the `AxonRequest::Graph(req) => handle_graph(cfg, req).await` arm.

- [ ] **Step 4: Verify compilation**

Run: `cargo check 2>&1 | tail -10`

- [ ] **Step 5: Commit**

```bash
git add crates/mcp/schema.rs crates/mcp/server/handlers_graph.rs crates/mcp/server.rs
git commit -m "feat(mcp): add graph action with build/status/explore/stats subactions"
```

---

### Task 14: Verify CLI Flag Wiring (Integration Check)

**Note:** The CLI flag parsing (`CliCommand::Graph`, `--graph` flag, `build_config.rs` mapping) was already done in Task 2, Step 2b. This task verifies the end-to-end wiring works.

- [ ] **Step 1: Verify `axon graph` dispatches correctly**

Run: `cargo run --bin axon -- graph 2>&1 | head -5`
Expected: Usage message showing `build|status|explore|stats|worker`

- [ ] **Step 2: Verify `--graph` flag is accepted by `ask`**

Run: `cargo run --bin axon -- ask "test" --graph 2>&1 | head -5`
Expected: Either works (if Neo4j not configured, falls back to vector-only with warning) or shows expected behavior

- [ ] **Step 3: Verify `axon graph build --help` shows usage**

Run: `cargo run --bin axon -- graph build 2>&1 | head -5`
Expected: Build starts or shows error about Neo4j URL

- [ ] **Step 4: Commit any fixes**

```bash
git add -A && git commit -m "fix: wire graph CLI flag integration"
```

---

## Chunk 5: Integration + .env + Docs

### Task 15: Embed Worker Auto-Enqueue for Graph

After the embed worker successfully upserts chunks for a URL, auto-enqueue a graph extraction job (if Neo4j is configured).

**Files:**
- Modify: `crates/jobs/embed.rs` (or wherever embed completion is handled) — add graph job enqueue

- [ ] **Step 1: Add graph enqueue after embed completion**

After successful embed upsert, add:
```rust
// Auto-enqueue graph extraction if Neo4j is configured
if !cfg.neo4j_url.is_empty() {
    if let Err(e) = enqueue_graph_job(pool, amqp_channel, &url, cfg).await {
        log_warn(&format!("graph auto-enqueue failed for {url}: {e}"));
        // Non-fatal — embed succeeded, graph extraction is optional
    }
}
```

- [ ] **Step 2: Implement `enqueue_graph_job` in `crates/jobs/graph.rs`**

```rust
pub async fn enqueue_graph_job(
    pool: &PgPool,
    channel: &lapin::Channel,
    url: &str,
    cfg: &Config,
) -> Result<uuid::Uuid, Box<dyn std::error::Error>> {
    let id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO axon_graph_jobs (url, config_json) VALUES ($1, $2) RETURNING id",
    )
    .bind(url)
    .bind(serde_json::json!({"collection": cfg.collection}))
    .fetch_one(pool)
    .await?;

    enqueue_job(pool, channel, &cfg.graph_queue, id).await?;
    Ok(id)
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check 2>&1 | tail -10`

- [ ] **Step 4: Commit**

```bash
git add crates/jobs/graph.rs crates/jobs/embed.rs
git commit -m "feat(embed): auto-enqueue graph extraction after embed completion"
```

---

### Task 16: Update `.env.example` + Documentation

**Files:**
- Modify: `.env.example` — add Neo4j + graph env vars
- Modify: `CLAUDE.md` — add graph command to command table and env var table

- [ ] **Step 1: Update `.env.example`**

Add after the existing queue config section:

```bash
# Neo4j (optional — enables graph features)
AXON_NEO4J_URL=                             # e.g., http://localhost:7474 — empty = graph disabled
AXON_NEO4J_USER=neo4j
AXON_NEO4J_PASSWORD=

# Graph extraction (optional, defaults shown)
AXON_GRAPH_QUEUE=axon.graph.jobs
AXON_GRAPH_CONCURRENCY=4
AXON_GRAPH_LLM_URL=http://localhost:11434   # Ollama endpoint
AXON_GRAPH_LLM_MODEL=qwen3.5:2b
AXON_GRAPH_SIMILARITY_THRESHOLD=0.75
AXON_GRAPH_SIMILARITY_LIMIT=20
AXON_GRAPH_CONTEXT_MAX_CHARS=2000
AXON_GRAPH_TAXONOMY_PATH=                   # Custom taxonomy JSON (empty = built-in)
```

- [ ] **Step 2: Update CLAUDE.md command table**

Add `graph` to the Commands table:

```markdown
| `graph <sub>` | Knowledge graph: build/status/explore/stats/worker. Requires `AXON_NEO4J_URL`. | Depends |
```

Add `--graph` to Global Flags:

```markdown
| `--graph` | flag | `false` | Enable graph-enhanced retrieval (requires Neo4j). |
```

- [ ] **Step 3: Commit**

```bash
git add .env.example CLAUDE.md
git commit -m "docs: add Neo4j and graph config to .env.example and CLAUDE.md"
```

---

### Task 17: Full Integration Test

End-to-end verification that the graph pipeline works.

- [ ] **Step 1: Verify `cargo check` passes**

Run: `cargo check 2>&1 | tail -10`
Expected: 0 errors

- [ ] **Step 2: Verify all existing tests pass**

Run: `cargo test --lib 2>&1 | grep "test result"`
Expected: All tests pass, 0 failures

- [ ] **Step 3: Run new graph tests**

Run: `cargo test graph -p axon_cli --lib 2>&1 | tail -30`
Expected: All taxonomy, similarity, extract, context, neo4j tests pass

- [ ] **Step 4: Verify clippy is clean**

Run: `cargo clippy 2>&1 | tail -10`
Expected: 0 warnings

- [ ] **Step 5: Verify `axon graph` command works**

Run: `cargo run --bin axon -- graph 2>&1`
Expected: Usage message with subcommands listed

- [ ] **Step 6: Commit any final fixes**

```bash
git add -A
git commit -m "test: verify full GraphRAG integration"
```

---

## Summary

| Task | Component | ~Lines | Dependencies |
|------|-----------|--------|--------------|
| 1 | Neo4j HTTP client | ~120 | None |
| 2 | Config fields | ~50 (across 5 files) | None |
| 3 | Qdrant type `id` field | ~20 | None |
| 4 | Graph job schema | ~80 | Task 2 |
| 5 | Layer 1: Taxonomy NER | ~300 | Task 4 |
| 6 | Layer 2: Similarity | ~200 | Task 3, 4 |
| 7 | Layer 3: LLM extraction | ~200 | Task 4 |
| 8 | GraphContext builder | ~200 | Task 1, 5 |
| 9 | Ask pipeline integration | ~50 | Task 2, 8 |
| 10 | Graph worker | ~200 | Tasks 4-7 |
| 11 | Graph services | ~150 | Task 1, 10 |
| 12 | Graph CLI command | ~200 | Task 2, 11 |
| 13 | MCP graph action | ~100 | Task 11 |
| 14 | CLI flag parsing | ~30 | Task 2 |
| 15 | Embed auto-enqueue | ~30 | Task 4 |
| 16 | .env + docs | ~40 | All |
| 17 | Integration test | ~0 | All |

**Total new code:** ~1,970 lines across 12 new files + 11 modified files.

**Parallelizable:** Tasks 1, 2, 3 are independent (Chunk 1). Tasks 5, 6, 7 are mostly independent (Chunk 2, but share Task 4). Tasks 11-14 depend on earlier tasks but are independent of each other.
