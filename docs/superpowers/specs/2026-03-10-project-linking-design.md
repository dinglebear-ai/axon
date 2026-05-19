# GraphRAG: Knowledge Graph Integration

**Date:** 2026-03-10
**Status:** Draft
**Scope:** Neo4j graph construction from Qdrant content, graph-enhanced retrieval, `axon graph` command

## Problem

Axon indexes 2.57M+ chunks from docs, GitHub repos, Reddit threads, and YouTube videos into Qdrant. Every chunk is an island — no relationships between them. A Tokio docs page and the Axon source code that imports Tokio are semantically related but structurally disconnected. The RAG pipeline (`axon ask`) returns whatever's closest in embedding space with no understanding of how content relates across sources.

Meanwhile, Neo4j already has 8,624 entities and 12,933 relationships from `save-to-md` sessions — projects, technologies, services, bugs, files, concepts — but this graph is completely disconnected from Qdrant search.

## Solution

GraphRAG: extract entities and relationships from indexed content into Neo4j, then use graph traversal at query time to scope and enrich vector search results.

Three extraction layers, each progressively more expensive:

1. **Regex + taxonomy** (Layer 1) — pattern matching for known tech entities (libraries, frameworks, CLI commands, file paths, import statements). Zero LLM cost, ~1000 docs/sec. Covers ~60-70% of entities.
2. **Embedding similarity** (Layer 2) — Qdrant recommend API finds semantically similar documents using existing vectors. Zero LLM cost, zero re-embedding. Creates `SIMILAR_TO` edges between documents.
3. **LLM extraction** (Layer 3) — Qwen 3.5 2B via local Ollama `/api/generate` with JSON schema enforcement (`format` parameter). ~72 tok/s locally. Handles the remaining ~20-30%.

Three new capabilities, decoupled from each other and from existing crawl/embed:

1. **Graph construction** (`axon graph build`) — three-layer extraction into Neo4j
2. **Graph context enrichment** (`axon ask --graph`) — vector search → extract entities from results → Neo4j 1-hop traversal → inject structured context into LLM prompt (option 3). Designed for forward-compatible upgrade to full chunk expansion (option 1) later.
3. **Graph exploration** (`axon graph explore`) — direct entity neighborhood queries against Neo4j

Same pattern as crawl → embed: each step is independent, has its own queue, its own worker, can be re-run without affecting the others. **All graph features are fully optional** — if `AXON_NEO4J_URL` is unset, every graph code path is a no-op. Existing crawl/embed/query/ask behavior is completely unchanged.

## Architecture

### Pipeline

```
crawl/scrape/ingest → AMQP → embed worker → Qdrant
                                                │
                                          AMQP  │  (new)
                                                ↓
                                         graph worker
                                          │         │
                              Layer 1:    │         │   Layer 2:
                           regex/taxonomy │         │   Qdrant recommend
                                          │         │   (embedding sim)
                                          ↓         │
                              ambiguous?──┐         │
                                yes       │no       │
                                ↓         │         │
                              Layer 3:    │         │
                           Qwen 3.5 2B    │         │
                           (Ollama)       │         │
                                ↓         ↓         ↓
                                       Neo4j
```

### Data Flow: Graph Construction

```
axon graph build [--url <url> | --domain <domain> | --all]
    │
    ├──→ Layer 1: Regex + taxonomy NER on chunk text (fast, no network)
    │       → extracts known tech entities, file paths, imports, CLI commands
    │
    ├──→ Layer 2: Qdrant recommend API (embedding similarity)
    │       → compute chunk_0 UUID from URL → POST /points/query with recommend
    │       → filter must_not same URL → top-K similar docs → SIMILAR_TO edges
    │
    ├──→ Layer 3: Qwen 3.5 2B via Ollama /api/generate (surgical, only ambiguous/complex)
    │       → retrieve full document from Qdrant (all chunks, sorted by chunk_index)
    │       → LLM extraction with think:false + format schema enforcement (GBNF grammar)
    │       → only invoked when Layer 1 finds ambiguous entities or unknown relationships
    │
    ├──→ Neo4j: MERGE entities, CREATE relationships, link to Qdrant point IDs
    └──→ Postgres: track extraction status (which URLs have been processed)
```

### Data Flow: Graph Context Enrichment (Option 3)

```
axon ask --graph "how does axon handle connection pooling?"
    │
    ├──→ Qdrant: vector search → top-K chunks (existing)
    ├──→ Extract entity names from top-K chunk payloads via taxonomy lookup (new)
    ├──→ Neo4j: 1-hop traversal from matched entities (new)
    │       → neighbors, relationship types, mention counts
    ├──→ Format GraphContext as structured text block (new)
    │       → cap at ~2000 chars, prioritize by relationship count
    ├──→ Prepend graph context to LLM prompt (new)
    └──→ LLM: answer grounded in vector chunks + graph context (existing, richer prompt)
```

**Forward path to Option 1 (chunk expansion):** The `GraphContext` struct also collects `neighbor_chunk_ids` from the traversal. In option 3, these IDs are unused. When we upgrade to option 1, `ask.rs` will use them to fetch additional chunks from Qdrant, merge with the original top-K, and rerank the combined set — zero change to the graph query layer.

### Data Flow: Graph Exploration

```
axon graph explore "Tokio"
    │
    ├──→ Neo4j: case-insensitive substring match on Entity.name (fuzzy lookup)
    ├──→ Neo4j: 1-hop traversal → all relationships grouped by type
    ├──→ Neo4j: MENTIONED_IN → count chunks + documents per entity
    ├──→ Neo4j: SIMILAR_TO → related documents with scores
    └──→ Format and display entity neighborhood summary
```

### Component Map

```
crates/
├── core/
│   └── neo4j.rs                  # NEW — thin HTTP client for Neo4j Cypher via reqwest
├── cli/commands/
│   └── graph.rs                  # NEW — axon graph build/status/explore/stats/worker
├── services/
│   └── graph.rs                  # NEW — service layer for graph operations
├── jobs/
│   ├── graph.rs                  # NEW — graph job table, enqueue, claim, mark
│   └── graph/
│       ├── taxonomy.rs           # NEW — Layer 1: regex + curated tech taxonomy NER
│       ├── taxonomy.json         # NEW — built-in tech taxonomy (248 entries, loaded via include_str!)
│       ├── similarity.rs         # NEW — Layer 2: Qdrant recommend API embedding similarity
│       ├── extract.rs            # NEW — Layer 3: LLM entity extraction via Ollama
│       ├── context.rs            # NEW — GraphContext builder (entity extraction from results, Neo4j traversal, formatting)
│       ├── worker.rs             # NEW — AMQP graph worker (orchestrates all 3 layers)
│       └── schema.rs             # NEW — Neo4j schema (node labels, relationship types, constraints)
└── vector/ops/
    ├── qdrant/types.rs           # MODIFY — add `id` field to QdrantPoint, QdrantSearchHit
    └── commands/
        └── ask.rs                # MODIFY — graph context injection when --graph is set
```

## Neo4j Client (`crates/core/neo4j.rs`)

Thin HTTP client over `reqwest` — same approach as the existing Qdrant client. No new crates. Hits Neo4j's HTTP transactional endpoint (`POST /db/neo4j/tx/commit`).

```rust
pub struct Neo4jClient {
    http: reqwest::Client,
    endpoint: String,  // from AXON_NEO4J_URL env var
    auth: Option<(String, String)>,
}

impl Neo4jClient {
    pub fn new(cfg: &Config) -> Option<Self>  // None if neo4j_url is empty
    pub async fn execute(&self, cypher: &str, params: Value) -> Result<()>
    pub async fn query(&self, cypher: &str, params: Value) -> Result<Vec<Value>>
    pub async fn health(&self) -> Result<bool>
}
```

All Cypher queries use parameterized `$variables` — no string interpolation of user input.

**Env vars:**
- `AXON_NEO4J_URL` (default: empty — Neo4j disabled if unset)
- `AXON_NEO4J_USER` (default: `neo4j`)
- `AXON_NEO4J_PASSWORD` (required if auth enabled)

**Failure mode:** If Neo4j is unreachable, graph operations fail with a clear error. Neo4j is required for `axon graph` commands — it is not optional for this feature. Other commands (`crawl`, `embed`, `query`, `ask` without `--graph`) are unaffected.

## Neo4j Schema

### Node Labels

```cypher
// Entity extracted from indexed content
(:Entity {
    name: String,           // canonical name (e.g. "Tokio", "axum::Router")
    entity_type: String,    // "technology", "project", "service", "concept", "person", "organization"
    description: String,    // LLM-generated one-line description
    created_at: DateTime,
    updated_at: DateTime
})

// Chunk reference — links Entity back to Qdrant point
(:Chunk {
    point_id: String,       // Qdrant point UUID
    url: String,            // source URL
    collection: String,     // Qdrant collection name
    chunk_index: Integer
})

// Source document — groups chunks
(:Document {
    url: String,            // source URL (unique)
    domain: String,
    source_type: String,    // "crawl", "github", "reddit", "youtube"
    extracted_at: DateTime, // when graph extraction last ran
    chunk_count: Integer
})
```

### Relationships

```cypher
// Entity relationships (extracted by Layer 1 taxonomy + Layer 3 LLM)
(e1:Entity)-[:RELATES_TO {relation: "USES"}]->(e2:Entity)
(e1:Entity)-[:RELATES_TO {relation: "IMPLEMENTS"}]->(e2:Entity)
(e1:Entity)-[:RELATES_TO {relation: "DEPENDS_ON"}]->(e2:Entity)
(e1:Entity)-[:RELATES_TO {relation: "PART_OF"}]->(e2:Entity)
(e1:Entity)-[:RELATES_TO {relation: "ALTERNATIVE_TO"}]->(e2:Entity)
(e1:Entity)-[:RELATES_TO {relation: "HOSTS"}]->(e2:Entity)

// Provenance — where was this entity mentioned?
(e:Entity)-[:MENTIONED_IN]->(c:Chunk)

// Document structure
(c:Chunk)-[:BELONGS_TO]->(d:Document)

// Embedding similarity (Layer 2 — Qdrant recommend API, no LLM)
(d1:Document)-[:SIMILAR_TO {score: 0.87}]->(d2:Document)
```

### Constraints and Indexes

```cypher
CREATE CONSTRAINT entity_name IF NOT EXISTS FOR (e:Entity) REQUIRE e.name IS UNIQUE;
CREATE CONSTRAINT document_url IF NOT EXISTS FOR (d:Document) REQUIRE d.url IS UNIQUE;
CREATE INDEX chunk_point_id IF NOT EXISTS FOR (c:Chunk) ON (c.point_id);
CREATE INDEX entity_type IF NOT EXISTS FOR (e:Entity) ON (e.entity_type);
```

### Integration with Existing Graph

The existing `save-to-md` entities (technology, service, project, file, bug, concept) remain untouched. The new `Entity` label is separate. Over time, we can create `SAME_AS` edges between them:

```cypher
// Future: link extracted Entity to existing save-to-md technology node
MATCH (e:Entity {name: "Tokio"}), (t:Entity {name: "Tokio", type: "technology"})
WHERE labels(t) <> labels(e)
MERGE (e)-[:SAME_AS]->(t)
```

This is a follow-up, not part of the initial build.

## Qdrant Changes

### Point ID Capture

`QdrantPoint` and `QdrantSearchHit` in `crates/vector/ops/qdrant/types.rs` currently drop the `id` field during deserialization. The graph worker needs point IDs to link chunks back to Neo4j.

**Current:**
```rust
pub struct QdrantPoint {
    pub payload: QdrantPayload,
}
```

**After:**
```rust
pub struct QdrantPoint {
    #[serde(default)]
    pub id: serde_json::Value,  // UUID string or integer — Qdrant returns both formats
    #[serde(default)]
    pub payload: QdrantPayload,
}

pub struct QdrantSearchHit {
    #[serde(default)]
    pub id: serde_json::Value,
    pub score: f64,
    pub payload: QdrantPayload,
}
```

Qdrant's scroll and search responses already include `id` in the JSON — this change captures what's already there.

### Entity ID Payload Field (Future)

After graph extraction, we could write extracted entity names back to Qdrant point payloads via `set_payload`:

```json
{
  "payload": { "entities": ["Tokio", "async runtime", "spawn"] },
  "filter": { "must": [{"key": "url", "match": {"value": "https://tokio.rs/..."}}] }
}
```

This would enable entity-based filtering directly in Qdrant without Neo4j at query time. Deferred — graph traversal via Neo4j is the primary path.

## Postgres Schema

```sql
CREATE TABLE IF NOT EXISTS axon_graph_jobs (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    url           TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'pending',    -- pending/running/completed/failed
    chunk_count   INTEGER DEFAULT 0,
    entity_count  INTEGER DEFAULT 0,
    relation_count INTEGER DEFAULT 0,
    config_json   JSONB,
    error_text    TEXT,
    created_at    TIMESTAMPTZ DEFAULT now(),
    updated_at    TIMESTAMPTZ DEFAULT now(),
    started_at    TIMESTAMPTZ,
    finished_at   TIMESTAMPTZ
);

CREATE INDEX idx_graph_jobs_status ON axon_graph_jobs(status);
CREATE INDEX idx_graph_jobs_url ON axon_graph_jobs(url);
```

Follows the same pattern as `axon_crawl_jobs`, `axon_embed_jobs`, etc. Auto-created via `ensure_schema()`.

## Extraction Layers

### Layer 1: Regex + Taxonomy (`crates/jobs/graph/taxonomy.rs`)

Pattern matching for structurally obvious entities. No LLM, no network, ~1000 docs/sec.

#### Pattern Categories

Five regex extractors run sequentially on each chunk's text. Each returns `EntityCandidate` structs:

```rust
pub struct EntityCandidate {
    pub name: String,           // canonical name (e.g. "tokio", "PostgreSQL")
    pub entity_type: String,    // from taxonomy or inferred from pattern
    pub confidence: f32,        // 0.0–1.0 — high for taxonomy hits, lower for regex-only
    pub source: CandidateSource, // which pattern found it
    pub ambiguous: bool,        // true → escalate to Layer 3
}

pub enum CandidateSource {
    Import,     // import/use statement
    FilePath,   // file system path
    CliCommand, // shell command
    Url,        // hyperlink
    Taxonomy,   // matched curated taxonomy entry
}
```

**1. Import/use statements** — Highest confidence. Language-specific patterns:

| Language | Pattern | Example | Extracted Entity |
|----------|---------|---------|-----------------|
| Rust | `use\s+(\w+)::` | `use tokio::sync::Mutex` | `tokio` (technology) |
| Python | `(?:from\s+(\w+)\s+import\|import\s+(\w+))` | `from fastapi import FastAPI` | `fastapi` (technology) |
| JavaScript/TS | `(?:import\s+.*\s+from\s+['"]([^'"]+)\|require\s*\(\s*['"]([^'"]+))` | `import React from 'react'` | `react` (technology) |
| Go | `"([\w./]+)"` inside import block | `"github.com/gin-gonic/gin"` | `gin` (technology) |
| Shell | `(?:^|\s)(?:apt-get\s+install\|brew\s+install\|cargo\s+install\|pip\s+install\|go\s+install)\s+([\w@/.-]+)` | `cargo install ripgrep` | `ripgrep` (technology) |

**2. File paths** — Medium confidence. Detects project structure references:

```
Pattern: (?:^|\s|`)((?:[\w.-]+/){2,}[\w.-]+(?:\.\w+)?)
Examples:
  crates/vector/ops/tei.rs      → "tei" (concept, from filename)
  src/components/Button.tsx     → "Button" (concept)
  docker-compose.yaml           → "docker-compose" (config)
```

File paths generate `concept` type by default — only promoted to `technology` if the filename matches a taxonomy entry.

**3. CLI commands** — Medium confidence. Matches known tool invocations:

```
Pattern: (?:^|\n)\s*(?:\$\s+)?(?:cargo|docker|pnpm|npm|yarn|pip|uv|kubectl|git|make|just)\s+\w+
Examples:
  $ cargo build --release       → "cargo" (technology)
  docker compose up -d          → "docker" (technology)
  pnpm install                  → "pnpm" (technology)
```

Only the tool name is extracted (not the full command). The tool name is matched against the taxonomy for type assignment.

**4. URLs** — Low confidence for entity extraction, but creates `MENTIONED_IN` provenance:

```
Pattern: https?://(?:www\.)?([^/\s]+)
Examples:
  https://tokio.rs/tokio/tutorial  → domain "tokio.rs" linked to chunk
  https://github.com/tokio-rs/axum → "axum" extracted from repo path
```

GitHub URLs get special parsing: `github.com/{org}/{repo}` extracts `repo` as a `project` entity.

**5. Known tech taxonomy** — Highest confidence. Case-insensitive lookup against curated JSON:

```json
{
  "entries": [
    {"name": "Tokio", "type": "technology", "aliases": ["tokio-rs"], "category": "async-runtime"},
    {"name": "PostgreSQL", "type": "service", "aliases": ["postgres", "pg", "psql"], "category": "database"},
    {"name": "Docker", "type": "technology", "aliases": ["docker-compose", "dockerfile"], "category": "container"},
    {"name": "React", "type": "technology", "aliases": ["reactjs", "react.js"], "category": "ui-framework",
     "ambiguous": true, "disambiguation_hint": "library/framework context vs verb 'react'"},
    {"name": "Neo4j", "type": "service", "aliases": ["neo4j-browser", "cypher"], "category": "graph-database"}
  ]
}
```

#### Taxonomy JSON Schema

```json
{
  "type": "object",
  "properties": {
    "version": {"type": "string"},
    "entries": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "name": {"type": "string"},
          "type": {"type": "string", "enum": ["technology", "service", "project", "concept", "person", "organization"]},
          "aliases": {"type": "array", "items": {"type": "string"}},
          "category": {"type": "string"},
          "ambiguous": {"type": "boolean", "default": false},
          "disambiguation_hint": {"type": "string"}
        },
        "required": ["name", "type"]
      }
    }
  }
}
```

The built-in taxonomy ships as an embedded `include_str!()` JSON (~500 entries covering major languages, frameworks, databases, tools). Override with `AXON_GRAPH_TAXONOMY_PATH` for custom domain-specific entities (e.g., internal project names, proprietary tools).

#### Ambiguity Detection

An entity is marked `ambiguous: true` when:

1. **Taxonomy flag** — Entry has `"ambiguous": true` (e.g., "React", "Spring", "Express" — common English words that are also tech names)
2. **Multiple type matches** — Same name appears in taxonomy under different types (rare — taxonomy should be curated to avoid this)
3. **No taxonomy match but regex-only** — Entity found by import/CLI pattern but not in taxonomy. Confidence is lower, and Layer 3 can confirm or reject.

Ambiguous candidates are bundled with the chunk text and sent to Layer 3 for LLM disambiguation. Unambiguous candidates (taxonomy hit with `ambiguous: false`) skip LLM entirely.

#### Source-Type Awareness

Layer 1 adapts its patterns based on the chunk's `source_type` payload field:

| Source Type | Primary Patterns | Notes |
|-------------|-----------------|-------|
| `crawl` (docs) | Taxonomy + URLs + CLI commands | Documentation mentions tech by name |
| `github` (source) | Imports + file paths + taxonomy | Structural patterns are strongest |
| `reddit` (discussion) | Taxonomy + URLs | Conversational mentions, more ambiguity |
| `youtube` (transcript) | Taxonomy only | VTT transcripts have no code structure |

#### Output

Layer 1 returns `Vec<EntityCandidate>` per chunk. These are:
- Deduplicated by normalized name within the same chunk
- Passed to Layer 3 only if `ambiguous == true`
- Written directly to Neo4j via `MERGE` if `ambiguous == false` and `confidence >= 0.8`

### Layer 2: Embedding Similarity (`crates/jobs/graph/similarity.rs`)

Uses existing Qdrant vectors to find semantically related documents. Zero LLM cost, zero re-embedding — leverages vectors already in the collection.

#### Deterministic Point IDs

Every chunk's Qdrant point ID is deterministic:

```rust
use uuid::Uuid;

/// Compute the Qdrant point ID for a given URL and chunk index.
/// Same URL + chunk_index always produces the same UUID.
pub fn chunk_point_id(url: &str, chunk_index: usize) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("{url}:{chunk_index}").as_bytes())
}
```

This means we can compute `chunk_0`'s point ID from a URL without querying Qdrant — critical for feeding point IDs into the recommend API.

#### Single-URL Recommend Flow

For one source URL:

1. Compute `chunk_0_id = chunk_point_id(url, 0)` — deterministic, no network call
2. Call Qdrant recommend API:

```json
POST /collections/{collection}/points/query
{
  "query": {
    "recommend": {
      "positive": ["<chunk_0_uuid>"],
      "strategy": "average_vector"
    }
  },
  "filter": {
    "must_not": [
      {"key": "url", "match": {"value": "<current_url>"}}
    ]
  },
  "score_threshold": 0.75,
  "limit": 20,
  "with_payload": ["url", "source_type", "title"]
}
```

3. Group results by `url` payload field — take the highest score per unique URL
4. Filter by score threshold (`AXON_GRAPH_SIMILARITY_THRESHOLD`, default: 0.75)
5. Write `SIMILAR_TO` edges to Neo4j:

```cypher
MERGE (d1:Document {url: $source_url})
MERGE (d2:Document {url: $target_url})
MERGE (d1)-[r:SIMILAR_TO]->(d2)
SET r.score = $score, r.updated_at = datetime()
```

**Why `average_vector` strategy:** It preprocesses the positive example into a single search vector, making it as fast as a regular search. `best_score` would give slightly better results but performance scales linearly with example count — not worth it for batch processing. `average_vector` with a single positive example is effectively "find vectors most similar to this one."

**Why `chunk_0` only (not all chunks):** Using the first chunk as the representative vector balances cost vs quality. Most documents' first chunk contains the title, introduction, and key concepts. Using all chunks would require N recommend calls per document. If needed later, we can try `chunk_0 + chunk_middle + chunk_last` (3 positives in one call).

#### Score Threshold Rationale

Default threshold: **0.75** (cosine similarity).

| Range | Meaning | Action |
|-------|---------|--------|
| ≥ 0.90 | Near-duplicate or same topic | Always create edge |
| 0.80–0.89 | Strongly related | Create edge |
| 0.75–0.79 | Related but broader | Create edge (at default threshold) |
| 0.60–0.74 | Weakly related | Skip (too noisy) |
| < 0.60 | Different topics | Skip |

The threshold is tunable via `AXON_GRAPH_SIMILARITY_THRESHOLD`. Start at 0.75 and tighten to 0.80 if the graph gets too dense.

#### Cross-Source Filtering

Optional filter to find relationships across source types:

```json
{
  "must_not": [
    {"key": "url", "match": {"value": "<current_url>"}},
    {"key": "source_type", "match": {"value": "<current_source_type>"}}
  ]
}
```

This finds, e.g., "GitHub repos related to this crawled documentation page" — valuable for linking docs to their implementations. Enabled via `--cross-source` flag on `axon graph build`. Default: off (search all source types).

#### Batch Processing

For bulk similarity computation across the entire collection:

```rust
/// Process all URLs in the collection, creating SIMILAR_TO edges.
pub async fn batch_similarity(
    cfg: &Config,
    neo4j: &Neo4jClient,
    urls: Vec<String>,
) -> Result<BatchSimilarityResult> {
    let chunk_size = 32; // URLs per batch request

    let results = stream::iter(urls.chunks(chunk_size))
        .map(|url_batch| {
            // Build batch recommend request
            let searches: Vec<QueryPoints> = url_batch.iter().map(|url| {
                let point_id = chunk_point_id(url, 0);
                // Each search: recommend from chunk_0, exclude same URL
                QueryPoints {
                    collection: cfg.collection.clone(),
                    query: RecommendQuery {
                        positive: vec![point_id],
                        strategy: AverageVector,
                    },
                    filter: must_not_url(url),
                    score_threshold: Some(cfg.graph_similarity_threshold),
                    limit: cfg.graph_similarity_limit as u32,
                    with_payload: Some(vec!["url", "source_type", "title"]),
                }
            }).collect();

            // POST /collections/{col}/query/batch
            qdrant_query_batch(cfg, searches)
        })
        .buffer_unordered(4) // 4 batch requests in flight = 128 recommend queries
        .collect::<Vec<_>>()
        .await;

    // Write edges to Neo4j
    for (url, similar_docs) in results.flatten() {
        write_similarity_edges(neo4j, &url, &similar_docs).await?;
    }

    Ok(result)
}
```

**Qdrant batch endpoint:** `POST /collections/{collection}/query/batch` accepts `{"searches": [...]}` — one array response per search. This amortizes connection overhead and allows Qdrant to optimize internally.

**Concurrency model:** `buffer_unordered(4)` sends 4 batch requests concurrently, each containing 32 recommend queries = **128 concurrent recommend queries**. At ~1ms per recommend (HNSW is fast with pre-indexed vectors), this processes **~100K URLs in ~15 minutes** including Neo4j write overhead.

#### Duplicate Edge Prevention

On re-run (e.g., after new content is embedded), the `MERGE` pattern in Cypher prevents duplicate edges:

```cypher
MERGE (d1)-[r:SIMILAR_TO]->(d2)
SET r.score = $score, r.updated_at = datetime()
```

`MERGE` matches existing edges — if `d1→d2` already exists, it updates the score rather than creating a duplicate. This makes re-runs idempotent.

**Edge cleanup on score drop:** If a previously-similar document becomes less similar after re-embedding (new content changed the vector), the re-run will update the score. If the score drops below threshold, the edge isn't created — but the old edge remains. Periodic cleanup:

```cypher
// Remove stale SIMILAR_TO edges older than 30 days that weren't refreshed
MATCH ()-[r:SIMILAR_TO]->()
WHERE r.updated_at < datetime() - duration('P30D')
DELETE r
```

This runs as part of `axon graph build --cleanup` (future work).

#### Output

Layer 2 returns `Vec<SimilarityEdge>` per source URL:

```rust
pub struct SimilarityEdge {
    pub source_url: String,
    pub target_url: String,
    pub score: f32,
    pub target_source_type: String,
}
```

These are written directly to Neo4j — no Layer 3 involvement needed. Similarity edges are purely embedding-based and don't require LLM validation.

### Layer 3: LLM Extraction (`crates/jobs/graph/extract.rs`)

**Model:** `qwen3.5:2b` via local Ollama (2B preferred over 4B — tighter extraction, fewer hallucinated entities, 60% faster with schema enforcement)

**Endpoint:** `http://localhost:11434/api/generate` (Ollama native generate API — NOT `/api/chat`)

**Why `/api/generate`:** The `/api/chat` endpoint ignores `format` schema constraints on Qwen 3.5 models — it produces free-form types/relations wrapped in markdown fences despite the schema. The `/api/generate` endpoint properly enforces GBNF grammar constraints from the JSON schema, guaranteeing valid structured output. For single-shot extraction (no multi-turn context needed), `generate` is the correct endpoint.

**Performance:** ~72 tok/s (2B) vs ~45 tok/s (4B), ~6 seconds per extraction (~450 tokens), ~2.5 GiB VRAM steady-state (2B)

**Critical parameters:**
- `"think": false` — Without this, Qwen 3.5 spends all tokens on internal reasoning and produces empty content
- `"format": {<JSON schema>}` — Constrains output to match the extraction schema via GBNF grammar. Enum values for entity types and relation types are enforced at the token level — the model cannot produce values outside the schema

**When invoked:** Only for documents where Layer 1 finds ambiguous entities (e.g., "React" — library or verb?) or where relationships between entities are unclear. Layer 1 entities that are unambiguous (e.g., "Docker", "PostgreSQL") skip LLM entirely.

### LLM Extraction Request

```json
POST http://localhost:11434/api/generate
{
  "model": "qwen3.5:2b",
  "think": false,
  "stream": false,
  "format": {
    "type": "object",
    "properties": {
      "entities": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "name": {"type": "string"},
            "type": {"type": "string", "enum": ["technology", "project", "service", "concept", "person", "organization"]}
          },
          "required": ["name", "type"]
        }
      },
      "relationships": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "source": {"type": "string"},
            "target": {"type": "string"},
            "relation": {"type": "string", "enum": ["USES", "IMPLEMENTS", "PART_OF", "DEPENDS_ON", "ALTERNATIVE_TO", "HOSTS"]}
          },
          "required": ["source", "target", "relation"]
        }
      }
    },
    "required": ["entities", "relationships"]
  },
  "options": {"num_predict": 2048, "temperature": 0.1},
  "prompt": "Extract entities and relationships from the given text. Output valid JSON.\n\nText: <document text>"
}
```

**Response field:** `response` (string) — raw JSON, no markdown fences, directly parseable via `serde_json::from_str`.

**Verified extraction quality** (qwen3.5:2b with format enforcement):
```json
{
  "entities": [
    {"name": "Axon", "type": "project"},
    {"name": "Rust", "type": "technology"},
    {"name": "Spider.rs", "type": "technology"},
    {"name": "Qdrant", "type": "technology"},
    {"name": "HuggingFace TEI", "type": "technology"},
    {"name": "PostgreSQL", "type": "technology"},
    {"name": "Tokio", "type": "technology"},
    {"name": "Neo4j", "type": "technology"}
  ],
  "relationships": [
    {"source": "Axon", "target": "Rust", "relation": "USES"},
    {"source": "Axon", "target": "Spider.rs", "relation": "USES"},
    {"source": "Axon", "target": "Qdrant", "relation": "USES"},
    {"source": "Axon", "target": "HuggingFace TEI", "relation": "IMPLEMENTS"},
    {"source": "Axon", "target": "PostgreSQL", "relation": "USES"},
    {"source": "Axon", "target": "Tokio", "relation": "USES"},
    {"source": "Neo4j", "target": "Axon", "relation": "PART_OF"}
  ]
}
```

**Model comparison** (same input, `/api/generate` with `format`):

| Metric | 2B | 4B |
|--------|-----|-----|
| Speed | 72 tok/s | 45 tok/s |
| Tokens | 446 | 870 |
| Time | 6.2s | 19.2s |
| Entities | 8 (tight, correct) | 16 (over-extracts concepts like "job state", "documents") |
| Schema compliance | Perfect | Perfect |
| Recommendation | **Winner** — tighter, faster | Over-generates noise entities |
```

### Entity Resolution

Same entity appears across multiple documents (e.g., "Tokio" in axon source and tokio.rs docs). Resolution strategy:

1. **Name normalization** — lowercase, strip common suffixes (`.rs`, `()`, `::new`)
2. **MERGE in Cypher** — `MERGE (e:Entity {name: $name})` naturally deduplicates
3. **Type conflicts** — if two documents call the same entity different types, keep the most specific one (e.g., "library" beats "concept")
4. **Description update** — latest extraction wins (SET on MERGE)

### Batch Processing

For large collections, the graph worker processes URLs in batches:

1. Query Postgres for URLs not yet extracted (no completed `axon_graph_jobs` row)
2. For each URL: retrieve from Qdrant → extract via LLM → write to Neo4j
3. Mark job completed with entity/relation counts

Configurable concurrency via `AXON_GRAPH_CONCURRENCY` (default: 4 — LLM is the bottleneck, not I/O).

## Commands

### `axon graph build [--url <url> | --domain <domain> | --all]`

Extract entities from indexed content and populate Neo4j.

```bash
# Single URL
axon graph build --url https://tokio.rs/tokio/tutorial

# All URLs for a domain
axon graph build --domain tokio.rs

# Everything in the collection (async, enqueues jobs)
axon graph build --all

# Check extraction status
axon graph status

# Run the worker
axon graph worker
```

**Flow for single URL (sync):**
1. `qdrant_retrieve_by_url(cfg, url)` → get all chunks, reconstruct full document
2. Send to LLM with extraction prompt → get entities + relationships JSON
3. Write to Neo4j: MERGE entities, CREATE relationships, CREATE chunk nodes with point IDs
4. Insert `axon_graph_jobs` row with counts

**Flow for `--all` (async):**
1. `qdrant_url_facets(cfg, limit)` → get all unique URLs
2. Filter out URLs with existing completed `axon_graph_jobs` rows
3. Enqueue remaining URLs as graph jobs via AMQP
4. Graph worker processes them

### `axon graph status`

Show extraction progress.

```
Graph Extraction Status:
  Total indexed URLs:     4,231
  Extracted:              1,847 (43.7%)
  Pending:                2,384
  Failed:                 0

  Neo4j:
    Entities:             12,493
    Relationships:        28,741
    Documents:            1,847
```

### `axon graph explore <entity>`

Explore an entity's neighborhood in the knowledge graph. Case-insensitive substring matching — `tokio` matches `Tokio`, `react` shows `React`, `React Native`, `React Testing Library`.

```bash
axon graph explore "Tokio"
```

```
█ Tokio (technology)
  async runtime for Rust

  Relationships (23):
    USED_BY (8):  axum, hyper, Axon, tonic, warp ...
    DEPENDS_ON (2): mio, socket2
    PART_OF (1): tokio-rs

  Mentioned in 47 chunks across 12 documents:
    tokio.rs/tokio/tutorial  (14 chunks)
    github.com/tokio-rs/tokio  (9 chunks)
    docs.rs/tokio  (8 chunks)
    ...

  Similar documents (SIMILAR_TO):
    0.91  async-std docs
    0.87  smol runtime docs
    0.83  futures-rs docs
```

**Cypher (fuzzy match + neighborhood):**
```cypher
// Step 1: Fuzzy entity lookup
MATCH (e:Entity)
WHERE toLower(e.name) CONTAINS toLower($query)
RETURN e ORDER BY size(e.name) LIMIT 10

// Step 2: Relationships grouped by type
MATCH (e:Entity {name: $name})-[r:RELATES_TO]-(neighbor:Entity)
RETURN r.relation AS relation, collect(neighbor.name) AS targets

// Step 3: Mention counts
MATCH (e:Entity {name: $name})-[:MENTIONED_IN]->(c:Chunk)-[:BELONGS_TO]->(d:Document)
RETURN d.url AS url, count(c) AS chunk_count ORDER BY chunk_count DESC

// Step 4: Similar documents
MATCH (d1:Document)-[r:SIMILAR_TO]->(d2:Document)
WHERE d1.url IN $entity_doc_urls
RETURN d2.url, r.score ORDER BY r.score DESC LIMIT 10
```

Works with `--json` flag for machine-readable output.

### `axon graph stats`

Neo4j graph statistics.

```bash
axon graph stats
# Entity types: technology(234), service(89), concept(412), project(56), ...
# Relationship types: USES(1203), IMPLEMENTS(456), DEPENDS_ON(234), ...
# Total entities: 12,493  |  Total relationships: 28,741
# Most connected: Tokio (89 relationships), Docker (67), PostgreSQL (54)
```

## Graph-Enhanced Retrieval

### Approach: Context Enrichment (Option 3)

The graph enriches the LLM prompt with structured entity/relationship context **without changing which chunks are retrieved**. The existing vector search pipeline is untouched — graph context is additive.

This is deliberately simpler than full chunk expansion (option 1). It carries zero risk of degrading retrieval quality, adds minimal latency (one Neo4j round-trip), and immediately improves answer quality by giving the LLM entity context it wouldn't otherwise have.

**Forward path to Option 1:** The `GraphContext` struct collects `neighbor_chunk_ids` during traversal but doesn't use them in option 3. When we're ready for option 1, `ask.rs` fetches those chunks from Qdrant, merges with the original top-K, and reranks — zero change to the graph query layer.

### Modified `axon ask` Pipeline

Current flow:
1. Embed query → Qdrant vector search → top-K chunks
2. Build context from chunks
3. LLM generates answer

New flow with `--graph` flag:
1. Embed query → Qdrant vector search → top-K chunks **(existing, unchanged)**
2. Extract entity names from top-K chunk text via taxonomy lookup **(new)**
   - Run each chunk's text through the taxonomy hash map (same as Layer 1)
   - Collect unique entity names across all top-K results
3. Neo4j: find entities and traverse 1 hop **(new)**
   ```cypher
   // Find entities matching extracted names
   MATCH (e:Entity)
   WHERE e.name IN $entity_names
   WITH e
   // 1-hop: direct neighbors + relationship types
   OPTIONAL MATCH (e)-[r:RELATES_TO]-(neighbor:Entity)
   WITH e, collect({name: neighbor.name, type: neighbor.entity_type, relation: r.relation}) AS neighbors
   // Mention counts
   OPTIONAL MATCH (e)-[:MENTIONED_IN]->(c:Chunk)-[:BELONGS_TO]->(d:Document)
   WITH e, neighbors, count(DISTINCT d) AS doc_count, count(c) AS chunk_count
   RETURN e.name AS name, e.entity_type AS type, e.description AS description,
          neighbors, doc_count, chunk_count
   ORDER BY size(neighbors) DESC
   ```
4. Format `GraphContext` as structured text block, capped at ~2000 chars **(new)**
   - Prioritize entities by: (1) number of relationships, (2) mention count
   - Truncate at char budget — whole entities only, never mid-entity
5. Prepend graph context to LLM prompt before chunk context **(new)**
6. LLM generates answer grounded in vector chunks + graph context **(existing, richer prompt)**

### `GraphContext` Struct

```rust
/// Result of graph context enrichment for ask --graph.
/// Designed for forward-compatibility with option 1 (chunk expansion).
pub struct GraphContext {
    /// Formatted text block for LLM prompt injection.
    /// Capped at AXON_GRAPH_CONTEXT_MAX_CHARS (~2000).
    pub context_text: String,

    /// Raw entities found via taxonomy match on top-K results.
    pub entities: Vec<GraphEntity>,

    /// Chunk point IDs from 1-hop graph neighbors.
    /// UNUSED in option 3 (context enrichment).
    /// Used in option 1 (chunk expansion) to fetch additional
    /// chunks from Qdrant and merge with the original top-K.
    pub neighbor_chunk_ids: Vec<String>,

    /// Similarity edges found for matched documents.
    pub similar_docs: Vec<SimilarityEdge>,
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
    pub relation: String,     // "USES", "DEPENDS_ON", etc.
    pub target_name: String,
    pub target_type: String,
}
```

### Graph Context Format

Prepended to the LLM prompt before the chunk context:

```
## Related Knowledge Graph Context

- **Tokio** (technology): async runtime for Rust
  - USED_BY: axum, hyper, Axon, tonic, warp
  - DEPENDS_ON: mio, socket2
  - MENTIONED_IN: 47 chunks across 12 documents
- **axum** (technology): web framework built on Tokio
  - USES: Tokio, tower-http, serde
  - PART_OF: tokio-rs ecosystem
  - MENTIONED_IN: 23 chunks across 8 documents
```

**Prioritization when over budget:**
1. Entities with most relationships first
2. Within equal relationship count, entities mentioned in most documents
3. Truncate at entity boundary — never cut mid-entity

**Token budget:** ~2000 chars default (`AXON_GRAPH_CONTEXT_MAX_CHARS`). At ~4 chars/token, this is ~500 tokens — small relative to the 120K char ask context window, but enough for 5-10 well-connected entities with their neighborhoods.

### Activation

```bash
# Explicit graph-enhanced retrieval
axon ask "how does connection pooling work?" --graph

# Future default: auto-enable when Neo4j is configured and has extracted entities
```

`--graph` is opt-in initially. `evaluate` gets graph support for free since it calls `ask` internally. Once the graph has sufficient coverage (>50% of indexed URLs extracted), it becomes the default when Neo4j is available.

### v1 Scope

Only `axon ask --graph` gets graph enrichment in v1. Future candidates for graph enrichment:
- `suggest` — coverage analysis ("Tokio mentioned 47 times but tokio.rs not fully indexed")
- `query` — annotate vector results with entity metadata
- `evaluate` — already inherits from `ask`
- `sources` / `domains` — entity counts per source/domain

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Neo4j unreachable | `axon graph *` commands fail with clear error. `axon ask` without `--graph` unaffected. `axon ask --graph` falls back to vector-only with warning. |
| Neo4j empty (no entities extracted yet) | `ask --graph` proceeds normally — taxonomy matches entity names from top-K results, Neo4j lookup returns nothing, `GraphContext.context_text` is empty, ask continues with vector-only results. No error, no warning. Graceful degradation by design. |
| LLM extraction returns invalid JSON | Unlikely with `format` schema enforcement (GBNF grammar constrains token generation). If Ollama truncates mid-JSON (max tokens hit), retry with lower `num_predict` or mark job failed. |
| LLM extraction returns empty entities | Mark job completed with entity_count=0. Valid — some pages have no extractable entities. |
| Entity name collision across types | Keep most specific type. Log warning. |
| Qdrant retrieve fails for a URL | Mark graph job failed. Does not affect other URLs. |
| Graph worker AMQP disconnect | Same reconnect backoff as embed worker (2s initial, 60s cap). |
| Point IDs change on re-embed | `MENTIONED_IN` edges become stale. `axon graph build --url <url>` re-extracts and replaces. |

## Embed Pipeline Integration (Auto-Enqueue)

After the embed worker successfully upserts chunks for a URL, it enqueues a graph extraction job for that URL on the graph queue. This means every newly embedded document automatically gets entity extraction — same as how crawl auto-enqueues embed jobs.

```rust
// In embed worker, after successful upsert:
if graph_queue_available {
    enqueue_graph_job(pool, amqp_channel, &url, &cfg).await?;
}
```

Guard: only enqueue if `AXON_NEO4J_URL` is set and the graph queue exists. Silent no-op otherwise — embed pipeline is unaffected if Neo4j isn't configured.

## Files to Create/Modify

### New Files
| File | Purpose | ~Lines |
|------|---------|--------|
| `crates/core/neo4j.rs` | Neo4j HTTP client (query, execute, health) | ~150 |
| `crates/cli/commands/graph.rs` | CLI handlers (build, status, explore, stats, worker) | ~300 |
| `crates/services/graph.rs` | Service layer (build_graph, graph_status, graph_explore, graph_stats) | ~200 |
| `crates/jobs/graph.rs` | Job table ops (ensure_schema, enqueue, claim, mark) | ~150 |
| `crates/jobs/graph/taxonomy.rs` | Layer 1: regex patterns + curated tech taxonomy, entity candidate extraction. **Shared:** `Taxonomy::extract_entities(text) -> Vec<EntityCandidate>` is the single lookup function used by both Layer 1 (graph construction) and `context.rs` (ask --graph entity extraction from top-K results). Never duplicated. | ~200 |
| `crates/jobs/graph/taxonomy.json` | Built-in tech taxonomy (248 entries: languages, frameworks, databases, tools) | ~400 |
| `crates/jobs/graph/similarity.rs` | Layer 2: Qdrant recommend API, batch similarity queries, SIMILAR_TO edge creation | ~180 |
| `crates/jobs/graph/extract.rs` | Layer 3: Ollama LLM extraction (prompt, parse, entity resolution) | ~250 |
| `crates/jobs/graph/context.rs` | GraphContext builder: entity extraction from results, Neo4j traversal, text formatting | ~200 |
| `crates/jobs/graph/worker.rs` | AMQP graph worker (consume, process, write to Neo4j) | ~200 |
| `crates/jobs/graph/schema.rs` | Neo4j schema setup (constraints, indexes) | ~60 |

### Modified Files
| File | Change |
|------|--------|
| `crates/vector/ops/qdrant/types.rs` | Add `id: serde_json::Value` to `QdrantPoint` and `QdrantSearchHit` |
| `crates/core/config/types/enums.rs` | Add `CommandKind::Graph` |
| `crates/core/config/types/config.rs` | Add `neo4j_url`, `neo4j_user`, `neo4j_password`, `graph_*` fields, `graph_enabled` |
| `crates/core/config/cli.rs` | Add `graph` subcommand parsing, `--graph` flag on ask |
| `lib.rs` | Add `Graph` to command dispatch |
| `crates/vector/ops/commands/ask.rs` | Graph context injection when `--graph` is set (prepend `GraphContext.context_text` to prompt) |
| `crates/services/types/service.rs` | Add `GraphBuildResult`, `GraphStatusResult`, `GraphExploreResult`, `GraphStatsResult` |
| `crates/jobs/embed/worker.rs` | Auto-enqueue graph job after embed completes (guarded by `AXON_NEO4J_URL`) |
| `crates/jobs/common/amqp.rs` | Add graph queue constant |
| `.env.example` | Add `AXON_NEO4J_*`, `AXON_GRAPH_*` vars |
| `crates/mcp/schema.rs` | Add `graph` action to MCP tool schema |

## Testing Strategy

TDD: RED → GREEN → REFACTOR for every module. Tests written before implementation.

| Module | Tests |
|--------|-------|
| **taxonomy.rs** | Regex pattern matching: Rust `use`, Python `import`/`from`, JS `import`/`require`, Go `import`, shell `install` commands. Taxonomy JSON parsing + lookup. Alias resolution. Ambiguity detection. Case-insensitive matching. Source-type filtering. Deduplication within a chunk. Confidence scoring. |
| **similarity.rs** | Deterministic point ID generation (`chunk_point_id` — same input = same UUID, different input = different UUID). Recommend request construction. `must_not` filter assembly. Score threshold filtering. URL grouping + dedup (multiple chunks from same URL → one edge with max score). `SimilarityEdge` struct construction. Cross-source filter toggle. |
| **extract.rs** | Ollama request construction (model, think, format schema, prompt). Response parsing (`response` field → `serde_json::from_str`). Entity resolution: name normalization (lowercase, strip suffixes). Type conflict resolution (most specific wins). Empty entity list handling. Truncated JSON handling (Ollama `num_predict` exceeded). |
| **context.rs** | `GraphContext` struct construction. Entity extraction from chunk text via taxonomy. Context text formatting with structured block layout. Char budget cap (~2000 chars). Entity prioritization (relationship count → mention count). Truncation at entity boundaries. Empty graph (no entities found) returns empty context_text. `neighbor_chunk_ids` populated but unused in option 3. |
| **neo4j.rs** | Cypher query construction with parameterized `$variables`. Auth header assembly. Response parsing (results array, errors array). Health check endpoint. Error propagation on connection failure. `new()` returns None when URL empty. |
| **schema.rs** | Constraint + index Cypher generation. Idempotent `IF NOT EXISTS` guards. |
| **worker.rs** | Layer orchestration: Layer 1 → conditional Layer 3 → Layer 2 (independent). Job status transitions (pending → running → completed/failed). Entity/relation count aggregation. |
| **graph.rs (jobs)** | `ensure_schema()` table creation. `enqueue_graph_job`. `claim_next_pending`. `mark_job_completed`/`mark_job_failed`. |
| **Point ID capture** | `QdrantPoint.id` and `QdrantSearchHit.id` deserialize correctly for UUID string and integer formats. Default to `Value::Null` when absent. |
| **explore** | Case-insensitive substring entity matching. Relationship grouping by type. Mention count aggregation across chunks/documents. Similar document listing with scores. `--json` output format. Empty result handling (entity not found). |
| **Integration** | `axon graph build --url` → Qdrant retrieve + extraction + Neo4j write. `axon ask --graph` → vector search + taxonomy entity extraction + Neo4j 1-hop + context injection. `axon graph explore` → fuzzy match + neighborhood display. Auto-enqueue: embed worker → graph job → graph worker processes it. |

## Configuration

| Env Var | Default | Description |
|---------|---------|-------------|
| `AXON_NEO4J_URL` | *(empty)* | Neo4j HTTP endpoint. Empty = graph features disabled. |
| `AXON_NEO4J_USER` | `neo4j` | Neo4j auth username |
| `AXON_NEO4J_PASSWORD` | *(empty)* | Neo4j auth password |
| `AXON_GRAPH_QUEUE` | `axon.graph.jobs` | AMQP queue for graph extraction jobs |
| `AXON_GRAPH_CONCURRENCY` | `4` | Parallel graph extraction jobs per worker |
| `AXON_GRAPH_LLM_URL` | `http://localhost:11434` | Ollama API endpoint for Layer 3 extraction |
| `AXON_GRAPH_LLM_MODEL` | `qwen3.5:2b` | Ollama model for entity/relationship extraction (2B preferred — tighter extraction with schema enforcement) |
| `AXON_GRAPH_SIMILARITY_THRESHOLD` | `0.75` | Minimum cosine similarity for Layer 2 `SIMILAR_TO` edges |
| `AXON_GRAPH_SIMILARITY_LIMIT` | `20` | Max similar documents per URL in Layer 2 |
| `AXON_GRAPH_TAXONOMY_PATH` | *(built-in)* | Optional path to custom taxonomy JSON file |
| `AXON_GRAPH_CONTEXT_MAX_CHARS` | `2000` | Max chars for graph context injected into ask prompt |

## Open Questions (To Iterate)

### Resolved

1. ~~**LLM extraction model and approach**~~ → **Qwen 3.5 2B** via local Ollama `/api/generate` endpoint with `format` JSON schema enforcement. Three-layer pipeline: regex/taxonomy (Layer 1), embedding similarity (Layer 2), LLM only for ambiguity + relationships (Layer 3). Schema-guided extraction with fixed ontology (6 entity types, 6 relation types). `think: false` + `format` schema required. `/api/chat` does NOT enforce `format` constraints on Qwen 3.5 — must use `/api/generate`. 2B outperforms 4B for constrained extraction (tighter entities, 72 vs 45 tok/s).

2. ~~**Neo4j protocol**~~ → **HTTP transactional API** via `reqwest`. Endpoint configured via `AXON_NEO4J_URL` in `.env`. No new crates needed.

3. ~~**Retrieval approach**~~ → **Option 3: Context enrichment** (not chunk expansion). Extract entities from top-K vector results via taxonomy lookup → Neo4j 1-hop traversal → format as structured text block (~2000 chars) → prepend to LLM prompt. `GraphContext` struct carries both `context_text` (used now) and `neighbor_chunk_ids` (unused, ready for option 1 upgrade). 1 hop only, `ask --graph` only in v1, `graph explore` for direct graph queries.

### Open

4. **Entity resolution at scale** — Naive `MERGE` on name works for small graphs. At 2.57M chunks:
   - "React" the library vs "react" the verb — Layer 3 LLM handles disambiguation, but edge cases remain
   - Abbreviations and aliases ("TS" = "TypeScript", "PG" = "PostgreSQL") — taxonomy can map common aliases, but coverage is incomplete
   - Versioned entities ("React 18" vs "React 19" — same entity or different?) — current approach: same entity, no version tracking

5. **Graph staleness** — When content is re-embedded (updated docs), extracted entities become stale:
   - Full re-extraction on re-embed? Or diff-based?
   - How to garbage-collect orphaned entities with no remaining MENTIONED_IN edges?

6. **GPU resource contention** — TEI and Qwen 3.5 2B both need VRAM. TEI currently allocates ~11.8GB with aggressive batch settings on steamy-wsl. 2B model uses ~2.5 GiB VRAM (much less than 4B's ~5 GiB). If running locally (not on steamy-wsl), no contention. If co-located with TEI, tune TEI (`--max-concurrent-requests 64 --max-batch-tokens 32768`) or schedule graph extraction during off-peak embed times.

7. **Query logging for ranking** — Qdrant doesn't track query history. A Postgres query log table (`axon_query_log`) could record queries + result point IDs + user feedback, enabling query frequency as a ranking signal and taxonomy growth from real usage patterns. Deferred — not blocking for v1.

## Out of Scope (Future Work)

- **Option 1: Chunk expansion** — Use `GraphContext.neighbor_chunk_ids` to fetch additional chunks from Qdrant, merge with original top-K, and rerank. The struct is ready; `ask.rs` just needs to use the IDs.
- **`graph explore --to <entity>`** — Path exploration between two entities (shortest path / all paths). Natural extension of the neighborhood explorer.
- **`suggest` graph enrichment** — Coverage analysis: "Tokio mentioned 47 times but tokio.rs docs not fully indexed." Uses entity → document → URL mapping to find indexing gaps.
- **`query` graph enrichment** — Annotate vector search results with extracted entity metadata.
- **Query logging** — Postgres table recording queries + results + feedback for ranking signals and taxonomy growth.
- **`SAME_AS` bridging** — Linking extracted `Entity` nodes to existing `save-to-md` technology/service/concept nodes
- **Entity-based Qdrant filtering** — Writing entity names back to Qdrant payloads via `set_payload` for direct filtering without Neo4j
- **Community detection** — Clustering related entities into topic communities (Microsoft GraphRAG pattern)
- **Incremental re-extraction** — Re-extracting only changed chunks when a URL is re-embedded
- **MCP graph tool** — Exposing graph operations through the MCP server
- **Web UI** — Graph visualization in the Pulse workspace
- **Auto-enable `--graph`** — Making graph-enhanced retrieval the default when Neo4j has sufficient coverage
