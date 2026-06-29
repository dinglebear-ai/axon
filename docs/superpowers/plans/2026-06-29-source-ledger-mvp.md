# SourceLedger MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first working SourceLedger lifecycle slice: generic SQLite lifecycle state, sealed vector payload handoff, local embed/code-search refresh, crawl manifest commit semantics, one Git ingest adapter, durable status/backoff, and final command/docs cutover.

**Implementation correction:** The reviewed implementation keeps plain local
`axon embed /path` on the existing inline embed path and routes `embed --watch`
to the existing `axon-code-index` watcher for Git checkouts/workspaces.
SourceLedger covers the generic store, sealed payload fields, crawl generation
commits, mutable Git ingest manifests, status/backoff, and cleanup debt. Local
code-index unification remains future work, not part of this merged MVP slice.

**Architecture:** SourceLedger is a small source-kind-agnostic lifecycle primitive stored in the existing Axon SQLite runtime DB. `axon-source-ledger` owns typed store operations, `axon-jobs` owns SQLx migration execution, `axon-services` orchestrates refreshes, and vector/ingest/crawl crates only build manifests or `SourceDocument` inputs. Qdrant remains the vector store and only receives sealed lifecycle payload fields plus source-specific retrieval metadata.

**Tech Stack:** Rust 2024, Tokio, SQLx SQLite migrations, Qdrant payload filters/indexes, existing Axon Cargo workspace crates, Beads, GitHub CLI.

## Global Constraints

- Do not store embeddings or chunk vectors in SQLite.
- Do not create a second unmanaged database or code-index-style imperative schema bootstrap.
- Do not expose raw `config_json`, headers, credential-bearing URLs, canonical local paths, or raw internal error chains through source status.
- Do not let arbitrary adapter `extra` set `source_*` lifecycle fields.
- Do not allocate a generation while dependency preflight is failing.
- Do not create a second refresh scheduler; filesystem notifications only mark dirty/request refresh.
- Do not keep `code-search-watch` as a normal command or alias.
- Do not update OpenAPI/generated clients unless public schema actually changes.
- After implementation is green, run mandatory independent reviews before final completion: `lavra-review`, three `code_simplifier` passes, all available `pr-review-toolkit` roles, and PR comment resolution.

---

## File Structure

- Create `crates/axon-source-ledger/`: typed SourceLedger store, source IDs, manifest diffing, generations, cleanup debt, redacted status DTO support.
- Modify root `Cargo.toml`: add `crates/axon-source-ledger` as a workspace member and dependency where needed.
- Create `crates/axon-jobs/src/migrations/0017_source_ledger.sql`: append-only SQLx migration for `axon_source_*` tables.
- Modify `crates/axon-vector/src/ops/source_doc.rs`: reserve ledger-owned payload keys from arbitrary `extra`.
- Modify `crates/axon-vector/src/ops/tei/pipeline/payload.rs`: ensure ledger fields are applied only from sealed payload data.
- Modify `crates/axon-vector/src/ops/tei/qdrant_store/payload_indexes.rs`: add minimal source lifecycle indexes.
- Modify `crates/axon-vector/src/ops/qdrant/client/delete.rs`: add typed cleanup selector deletion helper.
- Modify `crates/axon-code-index/src/**`: adapt local code indexing to SourceLedger store operations.
- Modify `crates/axon-services/src/embed.rs`, `crates/axon-services/src/query.rs`, `crates/axon-services/src/code_search_watch.rs`: route local embed/code-search through ledger refresh, preflight, and status.
- Modify `crates/axon-cli/src/commands/embed.rs`, `crates/axon-core/src/config/cli.rs`, `crates/axon-core/src/config/help.rs`, parser tests: add `embed --watch` and tombstone `code-search-watch`.
- Modify `crates/axon-crawl/src/**` and `crates/axon-jobs/src/workers/runners/crawl.rs`: convert durable crawl manifests to SourceLedger manifests and commit only after embed/upsert.
- Modify `crates/axon-ingest/src/generic_git.rs` and related Git provider code: implement the first non-local mutable SourceLedger adapter for Git branch/file sources.
- Modify `crates/axon-api/src/**`, `crates/axon-services/src/system/status.rs`, and MCP/HTTP status surfaces only if public status schema is exposed.
- Modify docs in `CLAUDE.md`, `README.md`, `docs/reference/actions/{embed,code-search}.md`, API parity docs, and plugin skill docs after behavior exists.

---

### Task 1: SourceLedger Store And SQLx Migration

**Files:**
- Create: `crates/axon-source-ledger/Cargo.toml`
- Create: `crates/axon-source-ledger/src/lib.rs`
- Create: `crates/axon-source-ledger/src/store.rs`
- Create: `crates/axon-source-ledger/src/types.rs`
- Create: `crates/axon-source-ledger/src/status.rs`
- Create: `crates/axon-source-ledger/src/store_tests.rs`
- Create: `crates/axon-jobs/src/migrations/0017_source_ledger.sql`
- Modify: `Cargo.toml`
- Modify: `crates/axon-jobs/src/migrations/migration-checksums.txt` or repo-local checksum manifest if present

**Interfaces:**
- Produces: `SourceLedgerStore::new(pool: SqlitePool) -> Self`
- Produces: `SourceLedgerStore::acquire_lease(&self, source: &SourceIdentity, owner: &str, ttl_ms: i64) -> anyhow::Result<bool>`
- Produces: `SourceLedgerStore::preflight_refresh(&self, source_id: &str, now_ms: i64) -> anyhow::Result<RefreshPreflight>`
- Produces: `SourceLedgerStore::begin_generation(&self, source: &SourceIdentity) -> anyhow::Result<i64>`
- Produces: `SourceLedgerStore::diff_manifest(&self, source_id: &str, manifest: &[ManifestItem]) -> anyhow::Result<ManifestDiff>`
- Produces: `SourceLedgerStore::commit_generation(&self, source_id: &str, generation: i64) -> anyhow::Result<()>`
- Produces: `SourceLedgerStore::source_status(&self, source_id: &str) -> anyhow::Result<SourceStatus>`

- [ ] **Step 1: Write failing migration/store tests**

Add tests in `crates/axon-source-ledger/src/store_tests.rs`:

```rust
#[tokio::test]
async fn diff_manifest_reports_added_modified_removed_and_unchanged() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:").await.unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::LocalCode, "axon", 1);
    store.ensure_source(&source).await.unwrap();
    store.record_manifest_item("source-a", 1, ManifestItem::new("src/lib.rs", "hash-a", 10)).await.unwrap();
    store.commit_generation("source-a", 1).await.unwrap();

    let manifest = vec![
        ManifestItem::new("src/lib.rs", "hash-b", 11),
        ManifestItem::new("src/main.rs", "hash-c", 12),
    ];
    let diff = store.diff_manifest("source-a", &manifest).await.unwrap();

    assert_eq!(diff.modified[0].item_key, "src/lib.rs");
    assert_eq!(diff.added[0].item_key, "src/main.rs");
    assert_eq!(diff.removed, vec!["src/lib.rs".to_string()]);
}

#[tokio::test]
async fn preflight_backoff_blocks_generation_allocation() {
    let pool = axon_jobs::store::open_sqlite_pool(":memory:").await.unwrap();
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("source-a", SourceKind::LocalCode, "axon", 1);
    store.ensure_source(&source).await.unwrap();
    store.set_backoff("source-a", 10_000, "qdrant", "connection refused").await.unwrap();

    assert!(matches!(
        store.preflight_refresh("source-a", 1_000).await.unwrap(),
        RefreshPreflight::BackingOff { .. }
    ));
    assert_eq!(store.max_generation("source-a").await.unwrap(), 0);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p axon-source-ledger diff_manifest_reports_added_modified_removed_and_unchanged preflight_backoff_blocks_generation_allocation -- --nocapture`

Expected: FAIL because crate/types do not exist.

- [ ] **Step 3: Create the crate and migration**

Implement `types.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceKind {
    LocalCode,
    Crawl,
    Git,
    Feed,
    Session,
    Media,
}

#[derive(Debug, Clone)]
pub struct SourceIdentity {
    pub source_id: String,
    pub source_kind: SourceKind,
    pub collection: String,
    pub index_version: i64,
}

impl SourceIdentity {
    pub fn new(source_id: impl Into<String>, source_kind: SourceKind, collection: impl Into<String>, index_version: i64) -> Self {
        Self { source_id: source_id.into(), source_kind, collection: collection.into(), index_version }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestItem {
    pub item_key: String,
    pub content_hash: String,
    pub size_bytes: i64,
}

impl ManifestItem {
    pub fn new(item_key: impl Into<String>, content_hash: impl Into<String>, size_bytes: i64) -> Self {
        Self { item_key: item_key.into(), content_hash: content_hash.into(), size_bytes }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ManifestDiff {
    pub added: Vec<ManifestItem>,
    pub modified: Vec<ManifestItem>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshPreflight {
    Ready,
    BackingOff { until_ms: i64, dependency: String, message: String },
}
```

Create `0017_source_ledger.sql`:

```sql
CREATE TABLE IF NOT EXISTS axon_source_sources (
  source_id TEXT PRIMARY KEY,
  source_kind TEXT NOT NULL,
  collection TEXT NOT NULL,
  index_version INTEGER NOT NULL,
  committed_generation INTEGER NOT NULL DEFAULT 0,
  max_generation INTEGER NOT NULL DEFAULT 0,
  lease_owner TEXT,
  lease_expires_at_ms INTEGER NOT NULL DEFAULT 0,
  backoff_until_ms INTEGER NOT NULL DEFAULT 0,
  backoff_dependency TEXT,
  last_error TEXT,
  last_checked_at_ms INTEGER NOT NULL DEFAULT 0,
  last_success_at_ms INTEGER NOT NULL DEFAULT 0,
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS axon_source_manifest_items (
  source_id TEXT NOT NULL,
  item_key TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  indexed_generation INTEGER NOT NULL,
  pending INTEGER NOT NULL DEFAULT 0,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (source_id, item_key)
);

CREATE TABLE IF NOT EXISTS axon_source_cleanup_debt (
  source_id TEXT NOT NULL,
  generation INTEGER NOT NULL,
  item_key TEXT NOT NULL,
  selector_json TEXT NOT NULL,
  retry_count INTEGER NOT NULL DEFAULT 0,
  last_error TEXT,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (source_id, generation, item_key)
);

CREATE INDEX IF NOT EXISTS idx_axon_source_sources_kind ON axon_source_sources(source_kind);
CREATE INDEX IF NOT EXISTS idx_axon_source_sources_backoff ON axon_source_sources(backoff_until_ms);
CREATE INDEX IF NOT EXISTS idx_axon_source_cleanup_source ON axon_source_cleanup_debt(source_id);
```

- [ ] **Step 4: Implement minimal store methods**

Implement `store.rs` with SQLx queries matching the test names. Use one transaction inside `commit_generation`.

- [ ] **Step 5: Run targeted tests**

Run: `cargo test -p axon-source-ledger -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Validate layering**

Run: `cargo xtask check-layering`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/axon-source-ledger crates/axon-jobs/src/migrations
git commit -m "feat(source-ledger): add generic lifecycle store"
```

### Task 2: Sealed Ledger Payload And Cleanup Selectors

**Files:**
- Modify: `crates/axon-vector/src/ops/source_doc.rs`
- Modify: `crates/axon-vector/src/ops/tei/pipeline/payload.rs`
- Modify: `crates/axon-vector/src/ops/tei/qdrant_store/payload_indexes.rs`
- Modify: `crates/axon-vector/src/ops/qdrant/client/delete.rs`
- Test: `crates/axon-vector/src/ops/source_doc_tests.rs` or local sidecar test module

**Interfaces:**
- Consumes: `SourceIdentity`, `ManifestItem`
- Produces: `LedgerPayload::new(source_id: String, generation: i64, item_key: String, index_version: i64) -> LedgerPayload`
- Produces: `CleanupSelectorV1 { collection, source_id, source_index_version, source_generation, item_key }`

- [ ] **Step 1: Write spoofing and selector tests**

Test code:

```rust
#[test]
fn source_document_rejects_spoofed_ledger_extra() {
    let mut extra = serde_json::Map::new();
    extra.insert("source_id".into(), serde_json::json!("evil"));
    let err = SourceDocument::new("local://safe", "body", Some(extra)).unwrap_err();
    assert!(err.to_string().contains("ledger-owned payload key"));
}

#[test]
fn cleanup_selector_v1_rejects_empty_scope() {
    let selector = CleanupSelectorV1::new("axon", "", 1, 2, "src/lib.rs");
    assert!(selector.is_err());
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p axon-vector source_document_rejects_spoofed_ledger_extra cleanup_selector_v1_rejects_empty_scope -- --nocapture`

Expected: FAIL because types/validation do not exist.

- [ ] **Step 3: Implement reserved key validation**

Add a constant:

```rust
const LEDGER_OWNED_EXTRA_KEYS: &[&str] = &[
    "source_id",
    "source_kind",
    "source_generation",
    "source_item_key",
    "source_item_hash",
    "source_index_version",
];
```

Reject these keys in arbitrary `extra` before payload merge.

- [ ] **Step 4: Add minimal Qdrant indexes**

Add only these initially:

```rust
"source_id",
"source_kind",
```

Typed fields:

```rust
("source_generation", "integer"),
("source_index_version", "integer"),
```

Do not globally index `source_item_key` unless a later test proves it is required.

- [ ] **Step 5: Implement typed cleanup selector**

Add `CleanupSelectorV1` to the Qdrant delete module or a focused sibling module. It must fail if `collection`, `source_id`, or `item_key` is empty.

- [ ] **Step 6: Run targeted vector tests**

Run: `cargo test -p axon-vector source_document_rejects_spoofed_ledger_extra cleanup_selector_v1_rejects_empty_scope -- --nocapture`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/axon-vector
git commit -m "feat(vector): seal source ledger payload fields"
```

### Task 3: Local Embed And Code-Search Ledger Adapter

**Files:**
- Modify: `crates/axon-code-index/src/**`
- Modify: `crates/axon-services/src/embed.rs`
- Modify: `crates/axon-services/src/query.rs`
- Modify: `crates/axon-cli/src/commands/embed.rs`
- Modify: `crates/axon-core/src/config/cli.rs`
- Modify: `crates/axon-core/src/config/help.rs`
- Test: existing CLI/config/code-index tests

**Interfaces:**
- Consumes: `SourceLedgerStore`
- Produces: `ValidatedServerLocalPath`
- Produces: `embed --watch`
- Produces: tombstone/remap error for `code-search-watch`

- [ ] **Step 1: Write failing CLI and preflight tests**

Add parser tests:

```rust
#[test]
fn embed_accepts_watch_flag() {
    let cfg = parse_args(["axon", "embed", "/tmp/project", "--watch"]).unwrap();
    assert!(cfg.embed.watch);
}

#[test]
fn code_search_watch_returns_tombstone_error() {
    let err = parse_args(["axon", "code-search-watch"]).unwrap_err();
    assert!(err.to_string().contains("use `axon embed <path> --watch`"));
}
```

Add service test:

```rust
#[tokio::test]
async fn qdrant_down_does_not_allocate_local_generation() {
    let store = SourceLedgerStore::new(axon_jobs::store::open_sqlite_pool(":memory:").await.unwrap());
    store.set_backoff("local-source", 10_000, "qdrant", "connection refused").await.unwrap();
    let result = refresh_local_source_with_ledger(&store, "local-source").await;
    assert!(result.is_err());
    assert_eq!(store.max_generation("local-source").await.unwrap(), 0);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p axon-core embed_accepts_watch_flag code_search_watch_returns_tombstone_error -- --nocapture`

Expected: FAIL because `--watch` and tombstone are not wired.

- [ ] **Step 3: Add `--watch` to embed args**

Add `watch: bool` to `EmbedArgs`, parse `--watch`, and render help text: “Attach to SourceLedger refresh progress after registering this local path.”

- [ ] **Step 4: Add tombstone command handling**

Remove `code-search-watch` from normal help/dispatch. Preserve a recognizable tombstone error in parse or top-level unknown-command handling:

```text
code-search-watch was removed. Use `axon embed <path> --watch` to register and watch local code indexing.
```

- [ ] **Step 5: Add validated local-source registration**

Create a typed registration returned only by `validate_server_embed_input_with_config`. Store source id, allowed root, canonical path, display path, and collection. Revalidate before refresh.

- [ ] **Step 6: Route local refresh through SourceLedger preflight**

Before generation allocation, call `preflight_refresh`. If backing off, return a visible error/status and do not call `begin_generation`.

- [ ] **Step 7: Run targeted tests**

Run: `cargo test -p axon-core embed_accepts_watch_flag code_search_watch_returns_tombstone_error -- --nocapture`

Run: `cargo test -p axon-services qdrant_down_does_not_allocate_local_generation -- --nocapture`

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/axon-core crates/axon-cli crates/axon-services crates/axon-code-index
git commit -m "feat(embed): register local sources with source ledger"
```

### Task 4: Crawl Manifest Adapter

**Files:**
- Modify: `crates/axon-crawl/src/**`
- Modify: `crates/axon-jobs/src/workers/runners/crawl.rs`
- Modify: `crates/axon-services/src/crawl.rs`
- Test: crawl runner/service tests

**Interfaces:**
- Consumes: `SourceLedgerStore`
- Produces: crawl source manifest from durable `manifest.jsonl`

- [ ] **Step 1: Write failing crawl commit tests**

```rust
#[tokio::test]
async fn crawl_embed_failure_does_not_commit_generation() {
    let result = run_fixture_crawl_with_embed_failure().await;
    assert!(result.is_err());
    assert_eq!(ledger.committed_generation("crawl-source").await.unwrap(), None);
}

#[tokio::test]
async fn crawl_refresh_rejects_private_rebound_url() {
    let err = refresh_crawl_source_with_manifest_url("http://169.254.169.254/latest").await.unwrap_err();
    assert!(err.to_string().contains("blocked private address"));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p axon-jobs crawl_embed_failure_does_not_commit_generation crawl_refresh_rejects_private_rebound_url -- --nocapture`

Expected: FAIL because ledger adapter is missing.

- [ ] **Step 3: Build crawl manifest adapter**

After crawl completion, read final durable manifest entries, including 304/reused entries, into `ManifestItem { item_key: url, content_hash, size_bytes }`.

- [ ] **Step 4: Gate commit on embed/upsert success**

Only call `commit_generation` after the embed handoff reports success. On embed failure, persist status/error/backoff and leave generation uncommitted.

- [ ] **Step 5: Re-run URL/DNS/scope safety**

Before any automatic refresh fetches a stored URL, re-run DNS-aware URL validation plus existing scope/whitelist checks.

- [ ] **Step 6: Run targeted crawl tests**

Run: `cargo test -p axon-jobs crawl_embed_failure_does_not_commit_generation crawl_refresh_rejects_private_rebound_url -- --nocapture`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/axon-crawl crates/axon-jobs crates/axon-services
git commit -m "feat(crawl): commit source ledger generations after embed"
```

### Task 5: Git Branch Ingest Adapter

**Files:**
- Modify: `crates/axon-ingest/src/generic_git.rs`
- Modify: Git provider batch files under `crates/axon-ingest/src/github/**` if needed
- Modify: `crates/axon-services/src/ingest.rs`
- Test: ingest/generic git tests

**Interfaces:**
- Consumes: `SourceLedgerStore`
- Produces: mutable Git branch/file manifest adapter

- [ ] **Step 1: Write failing Git adapter tests**

```rust
#[tokio::test]
async fn git_branch_remove_creates_cleanup_debt_without_qdrant_scroll() {
    let diff = run_git_fixture_refresh(vec!["src/lib.rs"], vec![]).await.unwrap();
    assert_eq!(diff.removed, vec!["src/lib.rs"]);
    assert_eq!(ledger.cleanup_debt_count("git-source").await.unwrap(), 1);
}

#[tokio::test]
async fn immutable_commit_sha_does_not_schedule_refresh() {
    let source = register_git_commit_source("jmagar/axon", "0123456789abcdef0123456789abcdef01234567").await.unwrap();
    assert!(!source.refreshable);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p axon-ingest git_branch_remove_creates_cleanup_debt_without_qdrant_scroll immutable_commit_sha_does_not_schedule_refresh -- --nocapture`

Expected: FAIL because Git SourceLedger adapter is missing.

- [ ] **Step 3: Implement mutable Git branch manifest**

Use branch/repo/path/content hash as manifest identity. Keep `git_*` payload fields as retrieval metadata only.

- [ ] **Step 4: Drive stale cleanup from ledger debt**

Do not use broad Qdrant scrolls to discover stale files for ledger-managed Git sources. Use typed cleanup debt from SQLite.

- [ ] **Step 5: Run targeted ingest tests**

Run: `cargo test -p axon-ingest git_branch_remove_creates_cleanup_debt_without_qdrant_scroll immutable_commit_sha_does_not_schedule_refresh -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/axon-ingest crates/axon-services
git commit -m "feat(ingest): track git branch manifests in source ledger"
```

### Task 6: Scheduler, Backoff, Progress, And Redacted Status

**Files:**
- Modify: `crates/axon-jobs/src/watch.rs`
- Modify: `crates/axon-jobs/src/workers/watch_scheduler.rs`
- Modify: `crates/axon-services/src/system/status.rs`
- Modify: `crates/axon-api/src/**` if public status DTOs are exposed
- Modify: `crates/axon-services/src/code_search_watch.rs` or replacement source refresh service
- Test: scheduler/status tests

**Interfaces:**
- Consumes: `SourceLedgerStore::preflight_refresh`
- Produces: `SourceStatus { source_id, source_kind, phase, committed_generation, active_generation, backoff_until_ms, last_error, cleanup_debt_count, updated_at_ms }`

- [ ] **Step 1: Write failing scheduler/status tests**

```rust
#[tokio::test]
async fn source_status_redacts_headers_and_local_paths() {
    let status = status_for_fixture_with_header_and_local_path().await.unwrap();
    let body = serde_json::to_string(&status).unwrap();
    assert!(!body.contains("Authorization"));
    assert!(!body.contains("Cookie"));
    assert!(!body.contains("/home/"));
}

#[tokio::test]
async fn watcher_event_storm_coalesces_to_one_refresh() {
    let count = run_100_file_events_for_one_source().await.unwrap();
    assert_eq!(count.refreshes_started, 1);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p axon-services source_status_redacts_headers_and_local_paths watcher_event_storm_coalesces_to_one_refresh -- --nocapture`

Expected: FAIL because status/coalescing do not exist.

- [ ] **Step 3: Implement one scheduler path**

Use existing watch/job lease primitives or SourceLedger lease rows. Filesystem watchers may only mark dirty/request refresh.

- [ ] **Step 4: Persist progress and redacted status**

Persist phase, active/committed generation, diff counts, batch progress, backoff, cleanup debt count, last error, and heartbeat. Build redacted DTOs from those fields only.

- [ ] **Step 5: Run targeted scheduler/status tests**

Run: `cargo test -p axon-services source_status_redacts_headers_and_local_paths watcher_event_storm_coalesces_to_one_refresh -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/axon-jobs crates/axon-services crates/axon-api
git commit -m "feat(source-ledger): add refresh scheduler and redacted status"
```

### Task 7: Final Command, Docs, And Schema Cutover

**Files:**
- Modify: `CLAUDE.md`
- Modify: `README.md`
- Modify: `docs/reference/actions/embed.md`
- Modify: `docs/reference/actions/code-search.md`
- Modify: `docs/reference/api-parity.md`
- Modify: plugin skill docs if present
- Modify: OpenAPI/generated clients only if Task 6 changed public schema
- Test: CLI/help/docs/schema tests

**Interfaces:**
- Consumes: all previous task behavior
- Produces: final user-facing docs/help surface

- [ ] **Step 1: Write failing docs/help tests**

```rust
#[test]
fn help_mentions_embed_watch_not_code_search_watch() {
    let help = render_help();
    assert!(help.contains("embed"));
    assert!(help.contains("--watch"));
    assert!(!help.contains("code-search-watch"));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p axon-core help_mentions_embed_watch_not_code_search_watch -- --nocapture`

Expected: FAIL until docs/help are cut over.

- [ ] **Step 3: Update user-facing docs**

Document:

```text
axon embed /path
```

registers local mutable files for SourceLedger refresh.

Document:

```text
axon embed /path --watch
```

attaches foreground progress to the same refresh machinery.

- [ ] **Step 4: Audit stale references**

Run: `rg "code-search-watch" CLAUDE.md README.md docs crates plugins`

Expected: only changelog/remap notes remain.

- [ ] **Step 5: Regenerate public schema only if changed**

If Task 6 changed HTTP/MCP schema, run the repo’s schema generation command and commit generated artifacts in the same commit. If no schema changed, do not touch generated clients.

- [ ] **Step 6: Run final verification**

Run:

```bash
cargo fmt --check
cargo xtask check-layering
cargo clippy --all-targets --all-features
cargo test
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add CLAUDE.md README.md docs crates plugins apps
git commit -m "docs(source-ledger): cut over local indexing workflow"
```

---

## Mandatory Post-Implementation Review And PR Gate

- [ ] Create PR immediately after implementation and green verification.
- [ ] Run `lavra-review` against the worktree/PR and fix every finding.
- [ ] Run three `code_simplifier` passes against touched implementation files, tests, and docs/config/generated surfaces; fix every finding.
- [ ] Run all available `pr-review-toolkit` agents; fix every finding.
- [ ] Fetch PR comments with the repo-local command or `gh` equivalent; fix and resolve every actionable comment after pushing the matching code/docs changes.
- [ ] Run final verification again after all review/comment fixes.
- [ ] Save a session note before final `git add .`.
- [ ] Final commit and push all remaining work.

## Self-Review

Spec coverage: Tasks cover CH1 through CH7 first-wave implementation, engineering-review amendments, mandatory review waves, and PR creation.

Placeholder scan: No TBD/TODO/fill-in placeholders remain. Commands and expected outcomes are explicit.

Type consistency: `SourceLedgerStore`, `SourceIdentity`, `ManifestItem`, `ManifestDiff`, `RefreshPreflight`, `LedgerPayload`, `CleanupSelectorV1`, `ValidatedServerLocalPath`, and `SourceStatus` are consistently named across tasks.
