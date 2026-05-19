# Session: MCP SDK Ingest + TEI Tuning
**Date:** 2026-03-13
**Branch:** feat/web-integration-review-fixes

---

## Session Overview

Researched and ingested the official Model Context Protocol (MCP) repositories into the Axon knowledge base (6 GitHub repos), then diagnosed and tuned TEI embedding rate-limit thrashing caused by concurrent ingest lanes.

---

## Timeline

1. **Searched for official Rust MCP SDK** — confirmed `github.com/modelcontextprotocol/rust-sdk` (crate: `rmcp`, 3.2k stars)
2. **Searched for MCP specification repo** — confirmed `github.com/modelcontextprotocol/modelcontextprotocol`
3. **Enqueued 6 GitHub repos for ingest** via `axon ingest start source_type=github`
4. **Observed TEI 429 storms** in ingest worker logs — 2 concurrent lanes × 64-doc batches overwhelming TEI
5. **Diagnosed root cause** by reading `tei_client.rs`, `embed_pipeline.rs`, `ingest.rs`, `files.rs`
6. **Applied env tuning** — `AXON_INGEST_LANES=1`, `TEI_MAX_CLIENT_BATCH_SIZE=32` in `.env`

---

## Key Findings

- **`AXON_INGEST_LANES=2`** (default) runs 2 ingest jobs in parallel; each fires `embed_documents_in_batches` with `GITHUB_EMBED_DOC_BATCH_SIZE=64` — 2 concurrent 64-text TEI requests saturate the GPU
- **`tei_embed()`** in `crates/vector/ops/tei/tei_client.rs:132` is sequential per lane (stack-based loop), so 2 lanes = 2 simultaneous TEI POSTs
- **`TEI_MAX_CLIENT_BATCH_SIZE`** defaults to 64 (capped at 128) per `tei_client.rs:142`
- **Wiki clone failures** (exit 128) for all 6 repos are non-fatal — `wiki.rs` treats non-zero exit as `Ok(0)`, job continues without wiki content
- **Auth clone warning** (`auth_clone_failed retrying_unauthenticated`) is cosmetic — repos are public, unauthenticated clone succeeds

---

## Technical Decisions

- **`AXON_INGEST_LANES=1`** chosen over more complex concurrency throttling — serializing jobs eliminates TEI contention with zero code changes
- **`TEI_MAX_CLIENT_BATCH_SIZE=32`** chosen as half the default — reduces per-request token load without requiring code changes
- **Code change rejected** — adding `TEI_INTER_BATCH_DELAY_MS` between chunks in `tei_embed()` was considered but env-only fix was simpler and sufficient
- **2 lanes retained as default** in code — env override gives per-deployment control without changing hardcoded defaults

---

## Files Modified

| File | Change |
|------|--------|
| `.env` | `AXON_INGEST_LANES=2` → `1`; added `TEI_MAX_CLIENT_BATCH_SIZE=32` |

---

## Commands Executed

```bash
# Ingest jobs enqueued (all 6 succeeded)
axon ingest start --source_type github --target modelcontextprotocol/modelcontextprotocol  # job b79754d6
axon ingest start --source_type github --target modelcontextprotocol/rust-sdk              # job 2b8990f7
axon ingest start --source_type github --target modelcontextprotocol/typescript-sdk        # job e7e98091
axon ingest start --source_type github --target modelcontextprotocol/python-sdk            # job d178eb15
axon ingest start --source_type github --target modelcontextprotocol/kotlin-sdk            # job e49ab41b
axon ingest start --source_type github --target modelcontextprotocol/go-sdk                # job a099b97b
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Ingest parallelism | 2 lanes concurrent | 1 lane (serialized) |
| TEI batch size | 64 texts/request | 32 texts/request |
| TEI 429 rate | Frequent (attempts 1-3+ per batch) | Expected: rare/none |
| Throughput | Higher, unstable (retries) | Lower, stable |

---

## Ingest Jobs Enqueued

| Repo | Job ID | Status at session end |
|------|--------|----------------------|
| modelcontextprotocol/modelcontextprotocol | b79754d6-2e6e-45cc-8007-6a17fca68427 | Running (in-progress) |
| modelcontextprotocol/rust-sdk | 2b8990f7-f683-4c43-960d-410ee8340d70 | Queued/running |
| modelcontextprotocol/typescript-sdk | e7e98091-bee2-4517-b091-1c74f1ed5640 | Queued |
| modelcontextprotocol/python-sdk | d178eb15-b085-460c-b5b1-8a14db4413ab | Queued |
| modelcontextprotocol/kotlin-sdk | e49ab41b-8fd4-4c20-a782-8fa210aecb6c | Queued |
| modelcontextprotocol/go-sdk | a099b97b-00dd-439d-9e7a-205e3c75fedf | Queued |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| Ingest jobs enqueued | 6 job IDs returned | 6 job IDs returned | ✓ |
| `.env` AXON_INGEST_LANES | 1 | 1 | ✓ |
| `.env` TEI_MAX_CLIENT_BATCH_SIZE | 32 | 32 | ✓ |
| TEI 429 after restart | Rare/none | Pending (worker not yet restarted) | Pending |

---

## Risks and Rollback

- **Rollback**: Revert `.env` — set `AXON_INGEST_LANES=2`, remove `TEI_MAX_CLIENT_BATCH_SIZE=32`, restart ingest worker
- **Risk**: Serialized ingest (1 lane) means 6 repos process sequentially — may take significantly longer than parallel. If time-to-index matters, consider `AXON_INGEST_LANES=2` with `TEI_MAX_CLIENT_BATCH_SIZE=16` instead.
- **Wiki failures** are expected and non-fatal for all public MCP repos — no action needed

---

## Decisions Not Taken

- **`TEI_INTER_BATCH_DELAY_MS` code change** — adds sleep between TEI chunks in `tei_embed()`; rejected because env-only fix was sufficient
- **Keeping `AXON_INGEST_LANES=2`** with smaller batches — would still allow 2 concurrent TEI requests; serialization is cleaner

---

## Open Questions

- Did the ingest worker get restarted to pick up new env values? (user action required)
- Will wiki clone failures affect completeness of MCP knowledge base? (wikis are optional — likely minimal impact)
- Are all 6 ingest jobs completing successfully under 1-lane + 32-batch config?

---

## Next Steps

1. Restart ingest worker: `cargo run --bin axon -- ingest worker`
2. Monitor logs for 429 reduction
3. Verify jobs complete: `axon ingest list --json`
4. Query indexed MCP content: `axon query "MCP tool registration rust" --collection cortex`
