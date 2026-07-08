# Memory Runtime Reference

`axon-memory` is the durable-memory domain: memory records and their full
lifecycle (remember, search, show, link, supersede, reinforce, contradict,
status/archive/forget, review, decay scoring, context assembly). It is a real,
substantially implemented crate (`crates/axon-memory/`, ~3,100 lines including
tests) — this page documents the actual `MemoryStore`/`SqliteMemoryStore`
behavior, not a future design.

See also: crate guide `crates/axon-memory/src/CLAUDE.md`, behavior contract
`docs/pipeline-unification/runtime/memory-contract.md`, action surface doc
`docs/reference/actions/memory.md` (CLI/REST/MCP transport parity, currently
narrower than the underlying crate — see "CLI/transport parity" below).

Memory is **not** a generic source adapter and does not own the vector store:
it builds indexing requests and consumes `axon-ledger`/`axon-graph` as
libraries, but the concrete Qdrant client and provider selection live outside
this crate (enforced by `cargo xtask check-layering`).

## MemoryStore trait

`crates/axon-memory/src/store.rs` defines the `MemoryStore` trait every caller
depends on. `remember`, `get`, `search`, `context`, `link`, and `reinforce` are
required. `supersede`, `contradict`, `set_status`, and `review` have default
`unsupported_option` implementations so a partial store can still satisfy the
trait, but **`SqliteMemoryStore` implements all of them** — the real store has
no unimplemented lifecycle operations. `FakeMemoryStore` (an in-memory
implementation used in tests across `axon-memory`, `axon-services`, etc.) also
implements the full set.

## Decay and scoring model

`crates/axon-memory/src/decay.rs` implements the score formula from the memory
contract exactly:

```text
base_score =
  0.45 * semantic_score +
  0.20 * confidence +
  0.15 * salience +
  0.10 * scope_match +
  0.10 * reinforcement_score

reinforcement_score = min(1.0, ln(1 + reinforcement_count) / 5.0)

decay_multiplier =
  1.0                                  when profile = none or pinned
  0.5 ^ (age_days / half_life_days)    otherwise

status_penalty =
  1.0   forgotten
  0.5   superseded
  0.25  archived (unless include_archived)
  0.0   otherwise (active/review/contradicted/working)

contradiction_penalty = 0.25 when status = contradicted, else 0.0

memory_score = clamp01(base_score * decay_multiplier - contradiction_penalty - status_penalty)
```

`DecayProfile` (`axon-api::source::memory`) has six half-lives:
`VeryFast` (1 day, working/session scratch), `Fast` (7 days, episode
summaries), `Normal` (30 days, facts/bugs/tasks/incidents), `Slow` (180 days,
decisions/procedures/entity profiles), `VerySlow` (730 days, durable
preferences/standing instructions), and `None` (infinite — pinned/manual
retention). Every `MemoryType` (`Decision`, `Fact`, `Preference`, `Task`,
`Bug`, `Procedure`, `Incident`, …) has a default decay profile via
`MemoryType::default_decay_profile()`; `resolve_profile` lets an explicit
`decay.profile` string on the record override that default. `age_days` is
measured from the most recent of `last_reinforced_at`/`updated_at`/
`created_at`, so reinforcement resets the decay clock.

Scoring is computed live at read time (`score_record`, called from `search`,
`reinforce`, `set_status`, etc. in `crates/axon-memory/src/sqlite.rs` and
`sqlite/lifecycle.rs`) — there is no background decay job that rewrites
stored scores; decay is a function of stored `decay` policy fields plus
current time, evaluated on demand.

## Lifecycle operations (`sqlite/lifecycle.rs`)

All of these are implemented against SQLite, not stubs:

- **`reinforce(memory_id, signal)`** — increments `decay.reinforcement_count`,
  sets `decay.last_reinforced_at = signal.timestamp` (resetting decay age),
  adjusts `salience` by `signal.amount` (clamped to `0.0..=1.0`), and appends
  a `MemoryHistoryEvent`.
- **`supersede`** — marks the old memory `Superseded`, points it at the
  replacement via `superseded_by`, and appends history. The old record and
  its links are preserved, never deleted — supersession is additive.
- **`contradict`** — flags **both** memories `Contradicted`, sets each
  record's `contradicts` field to point at the other, and appends history to
  both. Contradiction is symmetric.
- **`set_status`** — general status transition (`archive`, `forget`, `pin`
  is expressed via `decay.pinned` rather than a status, `review`, etc.).
  Transitioning to `MemoryStatus::Review` additionally calls `enqueue_review`,
  inserting an open row into the `memory_reviews` queue table.
- **`review`** — returns the review queue: memories with an open
  `memory_reviews` entry, filterable by `memory_type`/`scope`/`reason`
  substring, cursor-paginated.
- **`link`** — inserts a `memory_links` row (`link_type`, `target`,
  `confidence`, JSON `evidence`) and appends history. Idempotent link refresh
  and `relates_to`/`supersedes` link types are exposed at the CLI layer (see
  `docs/reference/actions/memory.md`).

`MemoryStatus` has seven states: `Active` (recallable), `Review` (needs
confirmation), `Superseded`, `Contradicted`, `Archived` (hidden but retained),
`Forgotten` (removed from recall, redacted/deleted per policy), and `Working`
(short-TTL scratch memory).

## Storage

SQLite is the metadata/graph-mirror store (`crates/axon-memory/src/sqlite.rs`,
schema in `crates/axon-memory/src/migrations/`); memory body content and
embeddings live in a dedicated Qdrant collection (`axon_memory` by default, or
`AXON_MEMORY_COLLECTION`). New memories are normalized through
`SourceDocument::new_memory(...)` before embedding so the Qdrant point ID is
the same deterministic UUID as the SQLite record, and the point carries the
shared planner fields (`chunk_content_kind`, `chunk_locator`, `source_range`)
used across all source-like domains. `axon-memory` builds these indexing
requests but does not own the concrete vector-store client.

## Graph mirror — marker only

Unlike decay/lifecycle, `crates/axon-memory/src/graph.rs` is genuinely a
**marker module**, not a real implementation: it defines
`MEMORY_GRAPH_REQUIRED_FACT = "memory_document"`,
`MEMORY_GRAPH_OPTIONAL_FACTS = ["memory_link", "supersedes"]`, and a
`memory_graph_candidates()` helper that returns a single-element candidate
list. There is no code here that actually writes to `axon-graph` yet — this
module documents the intended fact kinds a real graph-mirror implementation
would emit, but does not emit them. Treat `graph.rs` as the one part of this
crate that is still design-stage; everything else described on this page
(`decay.rs`, `sqlite.rs`, `sqlite/lifecycle.rs`, `sqlite/recall.rs`) is live.

## Context assembly

`context(request)` builds a bounded, source-cited context block: it searches
active memories matching a query/project/repo/file seed, joins their bodies,
and truncates to `token_budget` (estimated via whitespace-split word count,
not a tokenizer) if the assembled context exceeds it — truncation is recorded
in the result's `exclusions` list. This is what the CLI's `axon memory
context` and the Claude Code SessionStart recall hook
(`docs/reference/actions/memory.md` → "Claude Plugin SessionStart Recall")
both call through the service layer.

## CLI/transport parity

The lifecycle described above (reinforce/contradict/set_status/review) is
fully implemented in `axon-memory`, but `crates/axon-cli/src/commands/
memory.rs` currently only wires `remember`, `list`, `search`, `show`, `link`,
`supersede`, and `context` into the CLI's `clap` tree. This is a transport
wiring gap, not a missing capability — see
`docs/pipeline-unification/surfaces/command-contract.md` ("Memory Commands")
and Task 9 of
`docs/pipeline-unification/plans/2026-07-04-phase-3b-security-error-memory-completion.md`,
which tracks exposing the full lifecycle across CLI, MCP, and REST as one
contract.

## Testing

```bash
cargo test -p axon-memory
```

`crates/axon-memory/src/decay_tests.rs` covers the scoring formula directly;
`crates/axon-memory/src/sqlite_tests.rs` and `store_tests.rs` cover the SQLite
implementation and the store-contract behavior (supersession chains,
contradiction symmetry, review-queue enqueue/filter, reinforcement resetting
decay age) against both `SqliteMemoryStore` and `FakeMemoryStore`.
`crates/axon-memory/src/testing.rs` exposes fixtures for a stable record, a
superseded chain, decay scenarios, and context assembly for reuse by
downstream crates (`axon-services`, etc.).
