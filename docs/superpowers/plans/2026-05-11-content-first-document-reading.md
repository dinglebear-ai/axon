# Content-First Document Reading Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Axon's MCP and CLI document-reading flow content-first for client/server deployments: `scrape` should return useful inline content immediately, `retrieve` should become the canonical "read this document" API with pagination and refresh/fallback behavior, and `artifacts` should be repositioned as a debug/admin surface rather than the default way agents consume docs.

**Architecture:** Introduce a shared document windowing contract used by both `scrape` and `retrieve`, with a default 10k-token response budget plus continuation metadata. Rework `retrieve` to unify three backends — embedded Qdrant content, stored source markdown/html, and live scrape refresh — behind one response shape. Keep server-side artifacts for inspection and operations, but stop making server-local file paths the primary UX.

**Tech Stack:** Rust, Tokio, MCP action schema, existing `src/services/**` service boundary, Spider.rs scrape path, Qdrant retrieval, CLI + MCP shared types, docs/superpowers planning workflow.

---

## Locked Decisions

- Client/server mode is the default mental model for this work; server-local filesystem paths are implementation details, not the client contract.
- Default inline read budget is **10k tokens**. If exact tokenization is not already available, use a conservative approximation and return `token_estimate`.
- `scrape` and `retrieve` should converge on one paging contract rather than inventing separate continuation semantics.
- `retrieve` should become the canonical "read this known document" surface.
- `retrieve` may auto-scrape on miss or stale content because Spider is fast enough to make refresh-on-read practical.
- `artifacts` remains valuable for debug/ops, screenshots, raw payload inspection, and cleanup, but should no longer be the normal agent path for reading text documents.
- Do not push agents toward `artifacts` when `query`, `retrieve`, or `ask` would be a better fit.

## Current-State Findings

- MCP `scrape` is currently hard-forced to path mode via `InlineHint::AlwaysPath`.
- MCP `retrieve` is also currently hard-forced to path mode.
- The typed `RetrieveResult` already carries richer metadata than MCP currently exposes (`requested_url`, `matched_url`, `truncated`, `warnings`, `variant_errors`).
- `retrieve` already reconstructs full documents from Qdrant chunks and is closer to the right abstraction than `artifacts`.
- `artifacts.read` / `grep` / `head` are still useful, but they are better framed as admin/debug tools than as primary reading APIs.

## User Experience Direction

### Primary document flows

| Intent | Preferred surface |
|---|---|
| I just scraped a page and want the content now | `scrape` |
| I know the document URL and want the full page | `retrieve` |
| I want relevant passages across indexed docs | `query` |
| I want an answer synthesized from context | `ask` |
| I need raw file/debug/admin access | `artifacts` |

### Response principles

- Return inline content first.
- Include continuation metadata when content exceeds the budget.
- Return backend/source metadata so callers know whether content came from:
  - `qdrant`
  - `stored_source`
  - `live_scrape`
- Preserve warnings and failure visibility; do not silently hide stale/miss/refresh issues.

---

## Phase 1: Shared Document Windowing Contract

**Outcome:** `scrape` and `retrieve` share one content windowing and pagination model.

**Files likely affected:**
- `src/services/types/service.rs`
- `src/mcp/schema.rs`
- `src/mcp/server/common.rs`
- `src/mcp/server/handlers_query.rs`
- any new helper module for paged document responses

- [ ] Define the response envelope for paged document reads.
- [ ] Choose the continuation mechanism (`cursor` preferred if it can encode backend + position cleanly).
- [ ] Add metadata fields such as:
  - `content`
  - `truncated`
  - `token_estimate`
  - `next_cursor`
  - `remaining_tokens_estimate` or equivalent
  - `backend`
- [ ] Decide whether the budget is enforced by true tokenizer or conservative character approximation.
- [ ] Add unit tests around exact boundary behavior near the 10k-token cap.

## Phase 2: Make `scrape` Inline-First

**Outcome:** MCP `scrape` returns page content inline by default instead of forcing artifact-path mode.

**Files likely affected:**
- `src/mcp/server/handlers_query.rs`
- `src/mcp/server/artifacts/respond.rs`
- `src/services/scrape.rs`
- `src/mcp/schema.rs`

- [ ] Remove the unconditional path-only behavior for `scrape`.
- [ ] Return inline content up to the shared window budget.
- [ ] Include continuation metadata for the remainder of oversized pages.
- [ ] Keep artifact metadata secondary rather than primary.
- [ ] Ensure scrape responses still surface URL and relevant scrape metadata.
- [ ] Add tests proving default scrape behavior is inline-first and paginates when needed.

## Phase 3: Promote `retrieve` to the Canonical Reader

**Outcome:** `retrieve` becomes the main "read this document" API for known URLs.

**Files likely affected:**
- `src/services/query.rs`
- `src/services/types/service.rs`
- `src/mcp/server/handlers_query.rs`
- `src/cli/commands/retrieve.rs`
- `src/mcp/schema.rs`

- [ ] Expand `RetrieveResult` / MCP retrieve payload to expose:
  - `requested_url`
  - `matched_url`
  - `backend`
  - `truncated`
  - `warnings`
  - `variant_errors`
  - continuation metadata
- [ ] Replace MCP path-first retrieve responses with inline-first paginated document responses.
- [ ] Decide how CLI retrieve should render large content and continuation metadata.
- [ ] Add tests covering new retrieve payload shape and boundary cases.

## Phase 4: Source Fallback + Backend Unification

**Outcome:** `retrieve` can serve the best available document even when embeddings are missing or incomplete.

**Files likely affected:**
- `src/services/query.rs`
- retrieval helpers under `src/vector/ops/qdrant/**`
- source/output lookup code in crawl/scrape/job services
- shared types in `src/services/types/**`

- [ ] Implement fallback from Qdrant retrieval to stored source markdown/html.
- [ ] Normalize output so callers do not need different logic for Qdrant vs stored source.
- [ ] Add explicit `backend` / `source_kind` markers.
- [ ] Preserve warnings when one backend fails and another succeeds.
- [ ] Add tests for backend selection precedence.

## Phase 5: Auto-Refresh on Miss or Stale Content

**Outcome:** `retrieve` can auto-scrape when the current document copy is stale or absent.

**Files likely affected:**
- `src/services/query.rs`
- `src/services/scrape.rs`
- freshness/metadata storage for scraped sources
- `src/mcp/server/handlers_query.rs`

- [ ] Define what counts as **stale** and where freshness is stored.
- [ ] On retrieve miss, attempt a live scrape automatically.
- [ ] On stale content, decide whether retrieve returns fresh scrape content immediately and re-embeds synchronously or asynchronously.
- [ ] Surface refresh status explicitly in response metadata.
- [ ] Ensure refresh failures are visible and do not silently degrade correctness.
- [ ] Add tests for miss, stale, successful refresh, and failed refresh paths.

## Phase 6: Reposition `artifacts`

**Outcome:** `artifacts` remains useful, but no longer acts as the default doc-reading UX.

**Files likely affected:**
- `src/mcp/server/handlers_system.rs`
- `src/mcp/server.rs`
- `docs/MCP.md`
- `docs/MCP-TOOL-SCHEMA.md`
- CLI/docs/help surfaces mentioning artifact usage

- [ ] Update tool/help descriptions so agents are nudged toward `scrape`, `retrieve`, `query`, and `ask` for normal document consumption.
- [ ] Keep `artifacts` focused on:
  - list
  - grep/head/read for debugging
  - screenshots
  - cleanup/delete/admin operations
- [ ] Review whether a dedicated CLI artifact inspection command is needed, or whether better docs/help is sufficient.
- [ ] Verify no high-level docs still imply that raw artifact paths are the normal client contract.

## Phase 7: Docs, Schema, and Verification

**Outcome:** The new behavior is documented, test-covered, and understandable to operators and agents.

**Files likely affected:**
- `docs/MCP.md`
- `docs/MCP-TOOL-SCHEMA.md`
- relevant command docs under `docs/commands/**`
- tests across `src/mcp/schema/tests.rs`, service tests, and MCP handler tests

- [ ] Update schema/docs for paginated `scrape` and `retrieve` responses.
- [ ] Add tests for:
  - scrape inline default behavior
  - pagination boundaries around the 10k-token cap
  - retrieve backend selection (`qdrant`, `stored_source`, `live_scrape`)
  - stale/miss auto-refresh behavior
  - help text that de-emphasizes artifacts for normal reading flows
- [ ] Ensure docs clearly explain when to use `scrape`, `retrieve`, `query`, `ask`, and `artifacts`.

---

## Execution Order

1. Phase 1 — shared windowing contract
2. Phase 2 — inline-first scrape
3. Phase 3 — retrieve response shape
4. Phase 4 — source fallback
5. Phase 5 — auto-refresh
6. Phase 6 — artifact ergonomics/docs
7. Phase 7 — tests/schema/docs sweep

## Beads / Task Mapping

- `design-document-windowing-contract`
- `implement-inline-scrape-mcp`
- `extend-retrieve-response-shape`
- `implement-retrieve-source-fallback`
- `implement-retrieve-auto-refresh`
- `align-cli-and-help-ergonomics`
- `add-tests-and-docs`

## Notes And Risks

- A true 10k-token limit may require a tokenizer dependency or a well-documented approximation strategy.
- Auto-refresh changes `retrieve` semantics from "read indexed content" to "ensure readable content exists now"; response metadata must make that explicit.
- Refresh behavior must respect existing URL safety rules and should not silently embed low-quality or partial content.
- `artifacts` still matters for binary outputs and low-level debugging even if it stops being central to the text-reading UX.
