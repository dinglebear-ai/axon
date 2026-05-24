# Async Prepared Session Ingest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `axon sessions` work over HTTP server mode by decoding and redacting local session files on the client, uploading prepared documents to the server, persisting that upload durably, and embedding it asynchronously through the existing SQLite worker queue.

**Architecture:** Client-local decoders produce the same prepared embedding shape that local session ingest already uses. The server accepts a bounded prepared-session request, validates semantic quotas, persists the payload atomically with an ingest job through `ServiceContext.jobs`, not a parallel SQLite helper, and wakes the existing ingest workers. The worker loads the payload by job id, embeds in bounded batches, then deletes the persisted transcript payload after successful deserialization/completion.

**Tech Stack:** Rust, Axum, SQLx/SQLite, serde, existing `ServiceContext` job runtime, existing `vector::ops::embed_prepared_docs`, existing session decoders in `src/ingest/sessions/*`.

---

## Engineering Review Decisions

This plan has been revised after Lavra engineering review.

- Keep the async design.
- Do not add a cfg/pool-based enqueue helper. Payload enqueue must remain inside `ServiceContext.jobs` / `ServiceJobRuntime` so worker notification, pending queue caps, and `BEGIN IMMEDIATE` serialization are preserved.
- Add semantic quotas before enqueue: max docs, max total text bytes, max per-doc text bytes, max metadata bytes, allowed platforms, safe collection names, and reserved metadata keys.
- Delete persisted prepared payloads after successful worker completion and include payload rows in cleanup/clear behavior.
- Prove the large route really bypasses the current 128 KiB write-router body limit with a >128 KiB test.
- Reject legacy remote `source_type: "sessions"` centrally in request mapping so REST and MCP cannot accidentally scan server-local `~/.claude`, `~/.codex`, or `~/.gemini`.
- Defer `--incremental`, cursor status, plugin hooks, and MCP prepared upload until the core CLI/REST async path is shipped. MCP should only reject legacy sessions in this phase.
- `/v1/actions` is removed in current Axon and is not a surface for this work.

## File Structure

- Create: `src/ingest/sessions/prepared.rs` - wire DTO, validation, conversion between wire docs and existing prepared embedding docs.
- Modify: `src/ingest/sessions.rs` - expose decode-to-wire helpers, embed prepared session requests, and batch/group by collection.
- Modify: `src/ingest/sessions/{claude,codex,gemini}.rs` - reuse existing parsers while producing wire docs; fix Gemini to use bounded reads and redaction.
- Create: `src/jobs/migrations/0006_create_ingest_payloads.sql` - side table for uploaded payloads keyed by ingest job id.
- Modify: `src/jobs/backend.rs`, `src/services/runtime.rs`, `src/jobs/ops/enqueue.rs` - add runtime-backed `enqueue_with_sidecar` preserving shared pool, `BEGIN IMMEDIATE`, queue caps, and worker notification.
- Modify: `src/jobs/ingest/types.rs`, `src/jobs/config_snapshot.rs` - add `IngestSource::PreparedSessions`.
- Modify: `src/jobs/workers/runners/ingest.rs` - load sidecar payload, embed prepared sessions, delete payload on success.
- Modify: `src/services/ingest.rs` - add prepared-session start/execute service functions.
- Modify: `src/web/server/handlers/async_jobs.rs`, `src/web/server/routing.rs` - add `POST /v1/ingest/sessions/prepared` with correct large-body route layering and write auth.
- Modify: `src/services/ingest/request.rs` - reject legacy remote sessions centrally.
- Modify: `src/cli/commands/sessions.rs`, `src/cli/server_mode/plan.rs`, `src/cli/server_mode/plan_ingest.rs` - make sessions server mode perform client-local prep and upload instead of generic planning.
- Modify: `src/mcp/server/handlers_embed_ingest.rs`, `src/mcp/schema/requests.rs` - reject legacy MCP session ingest in this phase; prepared MCP upload is deferred.
- Modify: `docs/ingest/sessions.md`, `docs/commands/sessions.md`, `plugins/skills/axon/SKILL.md` - document the server-mode prepared upload path and current MCP limitation.

## Task 1: Prepared Session Wire Contract

**Files:**
- Create: `src/ingest/sessions/prepared.rs`
- Modify: `src/ingest/sessions.rs`
- Test: `src/ingest/sessions_tests.rs`

- [ ] **Step 1: Write the failing conversion and validation tests**

Add to `src/ingest/sessions_tests.rs`:

```rust
#[test]
fn prepared_session_doc_converts_to_prepared_doc_without_extra_override() {
    let doc = super::PreparedSessionDoc {
        url: "file:///home/me/.codex/sessions/2026/foo.jsonl".to_string(),
        title: Some("foo.jsonl".to_string()),
        text: "### USER:\nhello\n\n### ASSISTANT:\nworld".to_string(),
        session_platform: "codex".to_string(),
        session_project: Some("axon_rust".to_string()),
        session_date: Some("2026-05-23T20:19:38Z".to_string()),
        session_turn_count: Some(1),
        session_file: "/home/me/.codex/sessions/2026/foo.jsonl".to_string(),
        extra: serde_json::json!({
            "model": "gpt-5",
            "agent": "spoofed",
            "session_file": "/tmp/spoofed"
        }),
    };

    let prepared = doc.to_prepared_doc().expect("valid prepared doc");

    assert_eq!(prepared.source_type, "codex_session");
    assert_eq!(prepared.content_type, "text");
    let extra = prepared.extra.expect("extra metadata");
    assert_eq!(extra["agent"], "codex");
    assert_eq!(extra["session_file"], "/home/me/.codex/sessions/2026/foo.jsonl");
    assert_eq!(extra["project_name"], "axon_rust");
    assert_eq!(extra["model"], "gpt-5");
}

#[test]
fn prepared_session_request_rejects_oversized_total_text() {
    let mut cfg = Config::test_default();
    cfg.session_ingest_max_bytes = 1024;
    let request = super::IngestSessionsPreparedRequest {
        docs: vec![super::PreparedSessionDoc {
            url: "file:///tmp/a.jsonl".to_string(),
            title: None,
            text: "x".repeat(2048),
            session_platform: "claude".to_string(),
            session_project: None,
            session_date: None,
            session_turn_count: None,
            session_file: "/tmp/a.jsonl".to_string(),
            extra: serde_json::json!({}),
        }],
        project: None,
        collection: None,
    };

    let err = request.validate(&cfg).expect_err("oversized request");
    assert!(err.contains("total prepared session text exceeds"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test ingest::sessions::tests::prepared_session -- --nocapture`

Expected: compile failure because prepared DTOs do not exist.

- [ ] **Step 3: Add DTOs, conversion, and validation**

Create `src/ingest/sessions/prepared.rs`:

```rust
use crate::core::config::Config;
use crate::vector::ops::{PreparedDoc, chunk_text};
use serde::{Deserialize, Serialize};

const MAX_PREPARED_SESSION_DOCS: usize = 256;
const MAX_PREPARED_SESSION_METADATA_BYTES: usize = 64 * 1024;
const RESERVED_EXTRA_KEYS: &[&str] = &[
    "agent",
    "project_name",
    "session_date",
    "turn_count",
    "session_file",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PreparedSessionDoc {
    pub url: String,
    pub title: Option<String>,
    pub text: String,
    pub session_platform: String,
    pub session_project: Option<String>,
    pub session_date: Option<String>,
    pub session_turn_count: Option<u32>,
    pub session_file: String,
    #[serde(default)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IngestSessionsPreparedRequest {
    pub docs: Vec<PreparedSessionDoc>,
    pub project: Option<String>,
    pub collection: Option<String>,
}

impl IngestSessionsPreparedRequest {
    pub(crate) fn validate(&self, cfg: &Config) -> Result<(), String> {
        if self.docs.is_empty() {
            return Err("docs cannot be empty".to_string());
        }
        if self.docs.len() > MAX_PREPARED_SESSION_DOCS {
            return Err(format!(
                "too many prepared session docs: {} > {}",
                self.docs.len(),
                MAX_PREPARED_SESSION_DOCS
            ));
        }
        if let Some(collection) = &self.collection {
            validate_collection_name(collection)?;
        }
        let per_doc_limit = crate::ingest::sessions::session_ingest_max_bytes_for_config(cfg);
        let total_limit = per_doc_limit.saturating_mul(4);
        let mut total_text = 0usize;
        for doc in &self.docs {
            doc.validate(per_doc_limit)?;
            total_text = total_text.saturating_add(doc.text.len());
        }
        if total_text > total_limit {
            return Err(format!(
                "total prepared session text exceeds limit: {total_text} > {total_limit}"
            ));
        }
        Ok(())
    }
}

impl PreparedSessionDoc {
    pub(crate) fn validate(&self, per_doc_limit: usize) -> Result<(), String> {
        if self.text.trim().is_empty() {
            return Err("prepared session text is empty".to_string());
        }
        if self.text.len() > per_doc_limit {
            return Err(format!(
                "prepared session text exceeds per-doc limit: {} > {}",
                self.text.len(),
                per_doc_limit
            ));
        }
        match self.session_platform.as_str() {
            "claude" | "codex" | "gemini" => {}
            other => return Err(format!("unsupported session_platform: {other}")),
        }
        if !self.url.starts_with("file://") {
            return Err("prepared session url must use file://".to_string());
        }
        if self.session_file.trim().is_empty() {
            return Err("session_file is required".to_string());
        }
        let metadata_bytes = serde_json::to_vec(&self.extra)
            .map_err(|err| format!("invalid extra metadata: {err}"))?
            .len();
        if metadata_bytes > MAX_PREPARED_SESSION_METADATA_BYTES {
            return Err(format!(
                "prepared session metadata exceeds limit: {metadata_bytes} > {MAX_PREPARED_SESSION_METADATA_BYTES}"
            ));
        }
        Ok(())
    }

    pub(crate) fn to_prepared_doc(&self) -> Result<PreparedDoc, String> {
        self.validate(usize::MAX)?;
        let source_type = match self.session_platform.as_str() {
            "claude" => "claude_session",
            "codex" => "codex_session",
            "gemini" => "gemini_session",
            other => return Err(format!("unsupported session_platform: {other}")),
        };
        let mut extra = serde_json::Map::new();
        if let Some(obj) = self.extra.as_object() {
            for (key, value) in obj {
                if !RESERVED_EXTRA_KEYS.contains(&key.as_str()) {
                    extra.insert(key.clone(), value.clone());
                }
            }
        }
        extra.insert("agent".to_string(), self.session_platform.clone().into());
        extra.insert("project_name".to_string(), self.session_project.clone().into());
        extra.insert("session_date".to_string(), self.session_date.clone().into());
        extra.insert("turn_count".to_string(), self.session_turn_count.into());
        extra.insert("session_file".to_string(), self.session_file.clone().into());
        Ok(PreparedDoc {
            url: self.url.clone(),
            domain: "local".to_string(),
            chunks: chunk_text(&self.text),
            source_type: source_type.to_string(),
            content_type: "text",
            title: self.title.clone(),
            extra: Some(serde_json::Value::Object(extra)),
            extractor_name: None,
            structured: None,
        })
    }
}

fn validate_collection_name(value: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if valid {
        Ok(())
    } else {
        Err("collection contains unsupported characters".to_string())
    }
}
```

Modify `src/ingest/sessions.rs`:

```rust
mod prepared;
pub use prepared::{IngestSessionsPreparedRequest, PreparedSessionDoc};
```

- [ ] **Step 4: Run tests**

Run: `cargo test ingest::sessions::tests::prepared_session -- --nocapture`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ingest/sessions.rs src/ingest/sessions/prepared.rs src/ingest/sessions_tests.rs
git commit -m "feat(sessions): add prepared session upload contract"
```

## Task 2: Client Decode Uses Existing Session Shape

**Files:**
- Modify: `src/ingest/sessions.rs`
- Modify: `src/ingest/sessions/{claude,codex,gemini}.rs`
- Test: `src/ingest/sessions_decode_tests.rs`, `src/ingest/sessions/gemini_tests.rs`

- [ ] **Step 1: Write failing Gemini redaction and limit tests**

Add to `src/ingest/sessions/gemini_tests.rs`:

```rust
#[test]
fn gemini_parser_redacts_secret_like_tokens() {
    let content = serde_json::json!({
        "messages": [{
            "type": "user",
            "content": [{"text": "token sk-live-secret-1234567890abcd should vanish"}]
        }]
    })
    .to_string();

    let parsed = super::parse_gemini_json(&content).expect("parse gemini");

    assert!(parsed.contains("[redacted-secret]"));
    assert!(!parsed.contains("sk-live-secret"));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test ingest::sessions::gemini::tests::gemini_parser_redacts_secret_like_tokens -- --nocapture`

Expected: FAIL because Gemini appends raw text today.

- [ ] **Step 3: Apply redaction and bounded reads to Gemini**

In `src/ingest/sessions/gemini.rs`, replace raw file reads with `super::read_session_file_limited(&path).await?` and redact item text:

```rust
let content = super::read_session_file_limited(&path).await?;

if let Some(t) = item["text"].as_str() {
    combined.push_str(&super::redact_session_text(t));
    combined.push('\n');
}
```

- [ ] **Step 4: Add existing-shape conversion helpers**

In `src/ingest/sessions.rs`, add helpers that decode local files into existing `SessionDoc`, then convert that to the wire request grouped by collection:

```rust
pub async fn decode_local_sessions_to_prepared_requests(
    cfg: &Config,
    reporter: &PhaseReporter,
) -> Result<Vec<(String, IngestSessionsPreparedRequest)>, Box<dyn Error>> {
    let docs = collect_local_session_docs(cfg, reporter).await?;
    Ok(group_session_docs_for_upload(docs, cfg.sessions_project.clone()))
}

async fn collect_local_session_docs(
    cfg: &Config,
    reporter: &PhaseReporter,
) -> Result<Vec<SessionDoc>, Box<dyn Error>> {
    reporter.report_phase(PHASE_SCANNING).await;
    let multi = MultiProgress::new();
    let all_platforms = !cfg.sessions_claude && !cfg.sessions_codex && !cfg.sessions_gemini;
    let mut all_docs = Vec::new();
    if cfg.sessions_claude || all_platforms {
        all_docs.extend(claude::collect_claude_docs(cfg, &multi).await.unwrap_or_default());
    }
    if cfg.sessions_codex || all_platforms {
        all_docs.extend(codex::collect_codex_docs(cfg, &multi).await.unwrap_or_default());
    }
    if cfg.sessions_gemini || all_platforms {
        all_docs.extend(gemini::collect_gemini_docs(cfg, &multi).await.unwrap_or_default());
    }
    Ok(all_docs)
}
```

Add a `SessionDoc -> PreparedSessionDoc` conversion in one place so provider modules do not grow parallel DTO logic.

- [ ] **Step 5: Run focused session tests**

Run: `cargo test ingest::sessions -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/ingest/sessions.rs src/ingest/sessions/gemini.rs src/ingest/sessions_decode_tests.rs src/ingest/sessions/gemini_tests.rs
git commit -m "feat(sessions): prepare local sessions for upload"
```

## Task 3: Runtime-Backed Payload Enqueue

**Files:**
- Create: `src/jobs/migrations/0006_create_ingest_payloads.sql`
- Modify: `src/jobs/backend.rs`
- Modify: `src/services/runtime.rs`
- Modify: `src/jobs/ops/enqueue.rs`
- Test: `src/jobs/ops_tests.rs`, `src/services/ingest_tests.rs`

- [ ] **Step 1: Write failing notify-preserving enqueue tests**

Add tests that prove:

- Sidecar enqueue uses pending caps.
- Sidecar enqueue notifies workers through `ServiceContext.jobs`.
- The job row and sidecar row are committed atomically.

Use a capture runtime in `src/services/ingest_tests.rs` with:

```rust
async fn enqueue_with_sidecar(
    &self,
    payload: JobPayload,
    sidecar: JobSidecarPayload,
) -> BackendResult<Uuid> {
    self.seen.lock().unwrap().push((payload, sidecar));
    Ok(Uuid::nil())
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test enqueue_with_sidecar prepared_sessions -- --nocapture`

Expected: compile failure because `JobSidecarPayload` and runtime method do not exist.

- [ ] **Step 3: Add payload table**

Create `src/jobs/migrations/0006_create_ingest_payloads.sql`:

```sql
CREATE TABLE IF NOT EXISTS axon_ingest_payloads (
    job_id       TEXT PRIMARY KEY,
    payload_kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    FOREIGN KEY(job_id) REFERENCES axon_ingest_jobs(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_ingest_payloads_kind ON axon_ingest_payloads(payload_kind);
```

- [ ] **Step 4: Add sidecar payload type and runtime method**

In `src/jobs/backend.rs`:

```rust
#[derive(Debug, Clone)]
pub enum JobSidecarPayload {
    IngestPreparedSessions { payload_json: String },
}
```

In `ServiceJobRuntime`:

```rust
async fn enqueue_with_sidecar(
    &self,
    payload: JobPayload,
    sidecar: JobSidecarPayload,
) -> BackendResult<Uuid>;
```

Default test runtimes must implement this explicitly so missing support fails at compile time.

- [ ] **Step 5: Implement shared SQLite enqueue with sidecar**

In `src/jobs/ops/enqueue.rs`, add `enqueue_job_with_sidecar(pool, payload, sidecar, cfg)` that uses the same `begin_immediate`, `check_pending_cap_for`, `insert_payload`, `commit`, and rollback logic as `enqueue_job`. It must insert the sidecar inside the same transaction after the job row.

In `SqliteJobBackend`, implement:

```rust
pub async fn enqueue_with_sidecar(
    &self,
    payload: JobPayload,
    sidecar: JobSidecarPayload,
) -> BackendResult<JobId> {
    let kind = payload.kind();
    let id = ops::enqueue_job_with_sidecar(&self.pool, &payload, &sidecar, &self.cfg).await?;
    if let Some(ref workers) = self.workers {
        workers.notify(kind);
    }
    Ok(id)
}
```

- [ ] **Step 6: Run enqueue tests**

Run: `cargo test enqueue_with_sidecar -- --nocapture`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/jobs/migrations/0006_create_ingest_payloads.sql src/jobs/backend.rs src/services/runtime.rs src/jobs/ops/enqueue.rs src/jobs/ops_tests.rs src/services/ingest_tests.rs
git commit -m "feat(jobs): enqueue ingest sidecar payloads through runtime"
```

## Task 4: Prepared Sessions Worker Execution And Cleanup

**Files:**
- Modify: `src/jobs/ingest/types.rs`
- Modify: `src/jobs/config_snapshot.rs`
- Modify: `src/jobs/workers/runners/ingest.rs`
- Modify: `src/services/ingest.rs`
- Modify: `src/jobs/query.rs`
- Test: `src/jobs/workers/runners_tests.rs`, `src/services/ingest_tests.rs`, `src/jobs/query_tests.rs`

- [ ] **Step 1: Write failing worker tests**

Add tests proving:

- `IngestSource::PreparedSessions` round trips through `ingest_config_json`.
- Worker fails clearly if sidecar payload is missing.
- Worker deletes `axon_ingest_payloads` after successful prepared session execution.
- `cleanup_jobs` / `clear_jobs` remove sidecar rows for deleted ingest jobs.

- [ ] **Step 2: Add source variant**

Modify `src/jobs/ingest/types.rs`:

```rust
PreparedSessions {
    doc_count: usize,
    project: Option<String>,
},
```

Update `source_type_label` to return `"sessions_prepared"` and `target_label` to return `prepared:{doc_count}` or `prepared:{doc_count}:{project}`.

- [ ] **Step 3: Add execute service**

Add to `src/services/ingest.rs`:

```rust
pub async fn ingest_sessions_prepared_now(
    cfg: &Config,
    request: crate::ingest::sessions::IngestSessionsPreparedRequest,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
) -> Result<IngestResult, Box<dyn Error>> {
    request.validate(cfg).map_err(|err| format!("invalid prepared sessions request: {err}"))?;
    let chunks = crate::ingest::sessions::embed_prepared_session_request(cfg, request, progress_tx)
        .await?;
    Ok(map_ingest_result(serde_json::json!({
        "source": "sessions_prepared",
        "chunks": chunks,
    })))
}
```

Implement `embed_prepared_session_request` in `src/ingest/sessions.rs` so it converts docs, groups by collection, and calls `embed_prepared_docs` in bounded batches.

- [ ] **Step 4: Worker loads and deletes payload**

In `src/jobs/workers/runners/ingest.rs`, detect `PreparedSessions`, load sidecar by job id, deserialize, execute, and delete:

```rust
let payload_json: String = sqlx::query_scalar(
    "SELECT payload_json FROM axon_ingest_payloads WHERE job_id=? AND payload_kind='sessions_prepared'",
)
.bind(id.to_string())
.fetch_optional(pool)
.await?
.ok_or_else(|| format!("prepared sessions payload missing for job {id}"))?;

let request: crate::ingest::sessions::IngestSessionsPreparedRequest =
    serde_json::from_str(&payload_json)?;
let result = crate::services::ingest::ingest_sessions_prepared_now(
    &effective_cfg,
    request,
    Some(progress_tx.clone()),
)
.await
.map_err(lift_err)?;

sqlx::query("DELETE FROM axon_ingest_payloads WHERE job_id=?")
    .bind(id.to_string())
    .execute(pool)
    .await?;
return Ok(Some(result.payload));
```

If deserialization succeeds but embedding fails, keep the payload for retry/debug. Cleanup/clear handles later removal.

- [ ] **Step 5: Run worker tests**

Run: `cargo test jobs::workers services::ingest jobs::query -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/jobs/ingest/types.rs src/jobs/config_snapshot.rs src/jobs/workers/runners/ingest.rs src/services/ingest.rs src/ingest/sessions.rs src/jobs/query.rs src/jobs/workers/runners_tests.rs src/services/ingest_tests.rs src/jobs/query_tests.rs
git commit -m "feat(sessions): execute prepared session ingest jobs"
```

## Task 5: REST Endpoint With Correct Auth And Body Limits

**Files:**
- Modify: `src/web/server/handlers/async_jobs.rs`
- Modify: `src/web/server/routing.rs`
- Test: `src/web/server/handlers/rest_tests.rs`, `src/web/server/handlers/rest_auth_tests.rs`

- [ ] **Step 1: Write failing route tests**

Add tests for:

- `POST /v1/ingest/sessions/prepared` returns `202` and a job id.
- A body larger than 128 KiB but within the endpoint quota succeeds, proving route layering is correct.
- Oversized semantic payload returns `413` or `400` before enqueue.
- Loopback no-auth destructive blocking applies to `/v1/ingest/sessions/prepared`.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test prepared_sessions_endpoint -- --nocapture`

Expected: FAIL because route does not exist.

- [ ] **Step 3: Add endpoint handler**

Add to `src/web/server/handlers/async_jobs.rs`:

```rust
pub(crate) async fn start_prepared_sessions_ingest(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    Json(req): Json<crate::ingest::sessions::IngestSessionsPreparedRequest>,
) -> Result<impl IntoResponse, HttpError> {
    req.validate(&cfg).map_err(|err| HttpError::bad_request(&err))?;
    let outcome = services::ingest::ingest_sessions_prepared_start_with_context(
        &cfg,
        req,
        &state.service_context,
    )
    .await
    .map_err(HttpError::from_box)?;
    accepted_job("/v1/ingest", outcome.result.job_id)
}
```

- [ ] **Step 4: Add route with correct layering**

If a nested route-level limit still inherits the 128 KiB write-router limit, split the prepared route into its own protected write router with `DefaultBodyLimit::max(25 * 1024 * 1024)` applied at the route/router level that wins in Axum 0.8.

Also update destructive route detection:

```rust
for prefix in ["/v1/crawl", "/v1/embed", "/v1/extract", "/v1/ingest"] {
    if path == prefix || path.starts_with(&format!("{prefix}/")) {
        if *method == Method::POST {
            return true;
        }
    }
}
```

Preserve existing cleanup/recover/cancel handling.

- [ ] **Step 5: Run REST tests**

Run: `cargo test prepared_sessions_endpoint rest_auth -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/web/server/handlers/async_jobs.rs src/web/server/routing.rs src/web/server/handlers/rest_tests.rs src/web/server/handlers/rest_auth_tests.rs
git commit -m "feat(rest): accept prepared session ingest uploads"
```

## Task 6: CLI Server Mode Upload

**Files:**
- Modify: `src/cli/commands/sessions.rs`
- Modify: `src/cli/server_mode/plan.rs`
- Modify: `src/cli/server_mode/plan_ingest.rs`
- Test: `src/cli/server_mode_tests.rs`

- [ ] **Step 1: Replace generic sessions plan test**

Change the current test so `CommandKind::Sessions` bypasses generic REST planning:

```rust
let err = plan::server_rest_plan(&cfg).expect_err("sessions should bypass generic rest plan");
assert!(err.to_string().contains("sessions server mode is handled by the sessions command"));
```

- [ ] **Step 2: Add upload path**

In `src/cli/commands/sessions.rs`, when `AXON_SERVER_URL` is configured and the command is not a lifecycle subcommand:

1. Decode local session docs.
2. Build grouped `IngestSessionsPreparedRequest` values.
3. Validate each request locally.
4. POST each request to `/v1/ingest/sessions/prepared`.
5. Print returned job ids using existing server-mode rendering conventions.

Do not spawn local workers from this path.

- [ ] **Step 3: Keep local behavior**

Local `--wait true` still calls `ingest_service::ingest_sessions(cfg, None)`. Local default async behavior can continue to enqueue the old local `IngestSource::Sessions` job because it runs on the same filesystem; remote surfaces will reject that source.

- [ ] **Step 4: Run CLI tests**

Run: `cargo test cli::server_mode_tests -- --nocapture`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/cli/commands/sessions.rs src/cli/server_mode/plan.rs src/cli/server_mode/plan_ingest.rs src/cli/server_mode_tests.rs
git commit -m "feat(cli): upload prepared sessions in server mode"
```

## Task 7: Central Legacy Remote Sessions Rejection

**Files:**
- Modify: `src/services/ingest/request.rs`
- Modify: `src/mcp/server/handlers_embed_ingest.rs`
- Test: `src/services/ingest_tests.rs`, `src/mcp/server/handlers_embed_ingest_tests.rs`, `src/web/server/handlers/rest_tests.rs`

- [ ] **Step 1: Write failing rejection tests**

Add tests proving `source_type: "sessions"` is rejected for REST and MCP remote request mapping with:

```text
remote session ingest must upload prepared docs; run axon sessions from the client
```

- [ ] **Step 2: Centralize rejection**

In `src/services/ingest/request.rs`, make `source_from_mcp_request` reject `IngestSourceType::Sessions` when called for remote REST/MCP. If local CLI still needs this mapper for local behavior, split the function into `source_from_remote_request` and `source_from_local_request`.

- [ ] **Step 3: Run rejection tests**

Run: `cargo test remote_sessions_rejected -- --nocapture`

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/services/ingest/request.rs src/mcp/server/handlers_embed_ingest.rs src/services/ingest_tests.rs src/mcp/server/handlers_embed_ingest_tests.rs src/web/server/handlers/rest_tests.rs
git commit -m "fix(ingest): reject remote server-local session scans"
```

## Task 8: Focused End-To-End Verification

**Files:**
- Modify: `tests/client_server_mode.rs`
- Modify docs if behavior differs during implementation.

- [ ] **Step 1: Add focused integration test**

Add a test that starts the server with a temp SQLite data dir, posts a prepared session request through the CLI/server-mode path, polls `/v1/ingest/{job_id}`, and asserts completion with:

```json
{ "source": "sessions_prepared" }
```

Use a small prepared doc and test collection so it does not need real local `~/.claude` or `~/.codex`.

- [ ] **Step 2: Run focused integration tests**

Run: `cargo test client_server_mode -- --nocapture`

Expected: PASS.

- [ ] **Step 3: Run broad verification**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add tests/client_server_mode.rs
git commit -m "test(sessions): verify async prepared session ingest"
```

## Task 9: Documentation

**Files:**
- Modify: `docs/ingest/sessions.md`
- Modify: `docs/commands/sessions.md`
- Modify: `plugins/skills/axon/SKILL.md`
- Test: `tests/cli_help_contract.rs`

- [ ] **Step 1: Document server mode**

Add to `docs/ingest/sessions.md`:

```markdown
## HTTP server mode

In server mode, `axon sessions` scans local session files on the client, redacts text locally, uploads prepared docs to `POST /v1/ingest/sessions/prepared`, and the server embeds them asynchronously from a persisted job payload. The server does not read `~/.claude`, `~/.codex`, or `~/.gemini`.
```

- [ ] **Step 2: Document current non-goals**

State that plugin hooks, incremental cursor ingest, and MCP prepared uploads are follow-up work. MCP and REST `source_type: "sessions"` remote requests are rejected because they imply server-local filesystem scanning.

- [ ] **Step 3: Update plugin skill**

In `plugins/skills/axon/SKILL.md`, replace legacy remote session examples with CLI guidance:

```markdown
For local AI transcripts, run `axon sessions` on the machine that holds the transcripts. In server mode, the CLI prepares and uploads redacted docs; remote MCP/REST `source_type: "sessions"` is rejected.
```

- [ ] **Step 4: Run docs/help tests**

Run: `cargo test cli_help_contract -- --nocapture`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add docs/ingest/sessions.md docs/commands/sessions.md plugins/skills/axon/SKILL.md tests/cli_help_contract.rs
git commit -m "docs(sessions): document prepared server-mode ingest"
```

## Deferred Follow-Up Beads

Create separate beads after the core path is merged:

- `--incremental` cursor with completion-aware advancement or uploaded-pending reconciliation.
- Plugin `SessionStart` / `Stop` hooks. Hooks must no-op quietly when no server is configured; they must not run local synchronous embedding on session exit.
- MCP `sessions_prepared` upload support, with the same validation routine and explicit payload-size warning.
- Optional compressed artifact-backed payload storage for larger future uploads.
- Optional audit log event that records job id, doc count, collection, and byte counts without transcript text or local file paths.

## Self-Review

Spec coverage:
- Async prepared ingest is covered by Tasks 3, 4, 5, 6, and 8.
- Client-local decode and redaction are covered by Tasks 1 and 2.
- Legacy server-local sessions rejection is covered by Task 7.
- Hook-driven auto-capture is intentionally deferred because all reviewers identified cursor and hook safety risks.
- Documentation is covered by Task 9.

Risk controls:
- Uploaded transcripts are never acknowledged without a persisted sidecar payload tied atomically to a job row.
- Sidecar enqueue uses `ServiceContext.jobs` / runtime methods so worker notification, queue caps, shared pool, and transaction semantics are preserved.
- Semantic quotas prevent 25 MB transport bodies from exploding into unbounded chunks or metadata.
- Payloads are deleted after successful execution and are included in cleanup/clear paths.
- Route tests prove write auth, destructive loopback blocking, and large-body layering.
