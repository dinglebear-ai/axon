# Codex App-Server: Pooled Transport + Capability Upgrades

> **Status:** Plan. Epic `axon_rust-6a1r`. Supersedes the "desktop socket transport"
> deferred follow-up from `2026-06-14-codex-app-server-llm-backend.md`.

**Goal:** Make `AXON_LLM_BACKEND=codex-app-server` a first-class *preferred* synthesis
backend by (a) replacing the spawn-per-completion model with a bounded pool of
long-lived isolated app-server children, and (b) adopting the richer thread/turn
parameters the v2 protocol exposes (`ephemeral`, `developerInstructions`, `effort`,
`outputSchema`).

**Non-goal:** A full daemon / `unix://` / `ws://` socket transport. See "Why not the
daemon" below — the live benchmark says it would be over-engineering for synthesis.

---

## Live grounding (codex-cli 0.136.0, dookie, 2026-06-15)

Probed `codex app-server` directly (`/tmp/codex_isolated_probe.py`, mirroring
`src/core/llm/codex_app_server/home.rs` isolation):

| Measurement | Value | Source |
|---|---|---|
| Init handshake, **isolated** home (`apps=false, hooks=false`, no MCP/skills) | **~232 ms** | `ISOLATED_INIT_MS` |
| Init handshake, **real** `~/.codex` (loads skills/MCP) | **~10,150 ms** | first probe |
| Spawn → init → one turn → kill (full per-call cost) | **~3,791 ms** | `SPAWN_PER_CALL_TOTAL_MS` |
| Pooled steady-state turn (model-bound) | ~1.8–2.8 s | `pooled_turn{1,2,3}` |
| **Two+ turns on one process** | **works** (`REUSE_OK True`) | reuse probe |
| `ephemeral: true`, `developerInstructions` on `thread/start` | **accepted** | isolated probe |

**Authoritative protocol:** `codex app-server generate-json-schema --out <dir>` (v1 +
v2 bundles). Key shapes:
- `v2/ThreadStartParams`: `approvalPolicy, baseInstructions, config, cwd,
  developerInstructions, ephemeral, model, modelProvider, personality, sandbox,
  serviceTier, threadSource, ...`
- `v2/TurnStartParams`: `threadId, input, model, effort, personality, outputSchema,
  sandboxPolicy, serviceTier, summary, ...`
- `v2/ModelListResponse`, `v2/GetAccountRateLimitsResponse` — for doctor.

### What this changes about the original recommendation

The first review assumed init was the dominant per-call cost (the ~10 s figure). With
axon's isolation it is only **~232 ms**. So **pure latency from pooling is a modest
~300–500 ms/call win**, not seconds. The real payoffs are:

1. **Concurrency.** `codex_completion_concurrency` defaults to **1**, so today every
   synthesis call across the process is serialized. Fan-out workloads (research over N
   sources, `evaluate`'s RAG-vs-baseline + judge) pay this serially. A pool unlocks
   parallelism — this dwarfs the per-call init win.
2. **Capability correctness.** The current adapter folds `system_prompt` into the user
   prompt (`joined_prompt`) and ignores reasoning effort and structured output. The v2
   params fix all three properly.

This is why the slice is "pool + params," not "socket transport."

---

## Why not the daemon / socket transport

`codex app-server` ships `daemon` (managed long-lived server + control socket, built
for SSH-driven reuse), `proxy --sock`, and `--listen unix://|ws://`. Tempting, but for
axon synthesis it is the wrong tool:

- **Global/shared state.** The daemon is a per-user singleton with remote-control
  semantics and its own `~/.codex/app-server-daemon` lifecycle. Axon's whole isolation
  design (throwaway `CODEX_HOME`, no skills/MCP/hooks) fights that — we'd be reusing the
  user's *real* server (the 10 s init, skill-load errors we saw in stderr).
- **No measured benefit over an in-process pool.** Init is 232 ms isolated; a pool
  amortizes it to ~0. The daemon adds operational surface (start/stop/health, version
  skew, socket perms) for no extra synthesis throughput.
- **Isolation regression risk.** Connecting to a shared daemon reintroduces exactly the
  MCP/hooks/skill loading that `home.rs` exists to prevent.

Keep it as a *deferred* option only if a future **agentic** axon command needs a
persistent, tool-enabled Codex session — a different code path from synthesis.

---

## Architecture

```
core::llm::complete_streaming(req)              [unchanged facade]
  └─ CodexAppServer →
       CodexPool::get_or_spawn(backend)         [NEW: process pool keyed by (cmd, home-config, model)]
         ├─ CodexChild  (isolated CODEX_HOME, initialized once)   ── reused ──┐
         ├─ CodexChild                                                         │
         └─ … up to codex_completion_concurrency                              │
       child.run_turn(prompt, system, effort, output_schema)  ◄──────────────┘
         └─ protocol::TurnHandshake  [REFACTOR: split init (once) from per-turn]
```

- **One isolated `CODEX_HOME` per pool** (not per call): `home::prepare_codex_home`
  runs once; all children in the pool share it (read-only config + 0600 auth). The
  current per-call temp-dir churn goes away.
- **`CodexChild`** owns a spawned `codex app-server`, its `initialize`/`initialized`
  done once at spawn, a parsed `serverInfo`, and a mutex-guarded stdin/stdout for one
  in-flight turn at a time. `ephemeral: true` threads → no session files on disk.
- **Pool** hands out an idle child (or spawns up to the cap), runs one turn, returns it.
  Dead/erroring children are dropped and respawned. Idle children reaped after a TTL.
- **`protocol.rs` refactor:** today `CodexStreamState` couples `initialize` →
  `thread/start` → `turn/start` into one linear handshake. Split into
  `InitHandshake` (run once per child) and `TurnHandshake` (run per request, starts at
  `thread/start`). The pure state-machine style is preserved — still no process I/O in
  `protocol.rs`, still unit-testable without a child.

### Concurrency model — RESOLVED (bead `axon_rust-afdk`, probed 2026-06-15)

**One process serializes turns even across distinct `threadId`s.**
`/tmp/codex_concurrency_probe.py` started two ephemeral threads on one process and fired
both `turn/start`s simultaneously. Result `CONCURRENT_OVERLAP False`: thread A's deltas
ran 1.65–2.28 s, thread B's ran 2.36–3.02 s — strictly sequential, no interleave. (Also
confirmed: `threadId` is present on `item/agentMessage/delta`, useful for routing.)

**Decision:** the pool is **N independent processes, one in-flight turn each**. There is
no "1 process / M concurrent threads" shortcut. Pool size = `codex_completion_concurrency`.

---

## Task breakdown (maps to beads under epic `axon_rust-6a1r`)

### Task 0 — Concurrent-turns probe (`axon_rust-afdk`) — BLOCKS the pool
- Extend `/tmp/codex_isolated_probe.py`: start two threads on one process, dispatch
  `turn/start` for both, measure whether the second's deltas interleave with the first
  or only begin after `turn/completed` of the first.
- Output: a one-paragraph finding appended to this plan + the epic, choosing the pool
  shape. No production code.

### Task 1 — Richer thread/turn params (`axon_rust-k179`) — independent, do first
Smallest, highest-correctness-per-line change; ships value even before the pool.
- `protocol.rs`:
  - `thread/start` params: add `ephemeral: true`; move the system prompt from
    `joined_prompt` into `developerInstructions` (fall back to `baseInstructions` only if
    a probe shows `developerInstructions` is ignored — `developerInstructions` confirmed
    accepted live).
  - `turn/start` params: add `effort` derived from `LlmModelPurpose` /
    a new `cfg`-resolved knob (e.g. `low` for summarize, `medium` default, `high` for
    evaluate/judge). Keep `input` as the user prompt only (no longer the joined blob).
- `types.rs` / `codex_app_server.rs`: stop calling `joined_prompt`; thread
  `system_prompt` + an `effort` hint through `CodexStreamState::new`.
- Tests: `protocol_tests.rs` — assert `thread/start` carries `ephemeral` +
  `developerInstructions`, `turn/start` carries `effort` and a clean `input`.
- **Behavior-equivalence guard:** the assistant text extraction path
  (`agentMessage/delta` + `item/completed`) is unchanged.

### Task 2 — Bounded process pool (`axon_rust-31bx`) — depends on Task 0
- New `src/core/llm/codex_app_server/pool.rs` (+ `pool_tests.rs` sidecar). Monolith
  policy: keep < 500 lines; split child vs pool if needed.
- `CodexChild`: spawn + one-time init, `run_turn(...)`, liveness check, explicit
  `shutdown` (reuse the process-group SIGKILL from `codex_app_server.rs`).
- `CodexPool`: `LazyLock`/`OnceCell` keyed by `(codex_cmd, home fingerprint, model)`;
  semaphore-bounded to `codex_completion_concurrency`; idle TTL reaper; respawn on death.
- `complete_streaming` switches from "spawn child" to "borrow from pool." The whole
  `tokio::time::timeout` wrapper, stderr-tail diagnostics, and error redaction are
  preserved.
- **Raise the default** `codex_completion_concurrency` from 1 to a small N (e.g. 4),
  matching the Gemini default, now that init is amortized. Document in CLAUDE.md /
  configuration.md / env-matrix.
- Tests: pool reuse (same child across 2 turns), respawn after a killed child, cap
  enforcement, isolation preserved (no `HOME` leak — keep `home_tests.rs` invariants).

### Task 3 — Doctor/debug enrichment (`axon_rust-qsyh`) — independent
- In `validate_config`/doctor for Codex: after init, call `model/list` and
  `account/rateLimits/read`; surface available models + default reasoning efforts + ChatGPT
  rate-limit headroom. Today doctor only checks the binary is executable.
- Tests: parse fixtures captured from `generate-json-schema` v2 shapes.

### Task 4 — Structured output via `outputSchema` (`axon_rust-9i0q`) — depends on Task 2
- `turn/start.outputSchema` lets Codex return schema-valid JSON natively. Wire into the
  `extract` LLM-fallback and `evaluate` judge so we stop prompt-coaxing JSON.
- Lower priority; lands after the pool is stable.

---

## Verification

- `just verify` (fmt + clippy + check + test) at each task boundary.
- **Live smoke** (auth-gated, do not block merge on quota): `AXON_LLM_BACKEND=codex-app-server
  ./scripts/axon ask "<question>" --json` and a fan-out `research` to exercise pool
  concurrency. Record latency vs the spawn-per-call baseline.
- **Benchmark to capture:** wall-clock of a 5-source `research` synthesis at
  `codex_completion_concurrency=1` (today) vs the pool — this is the headline number,
  not single-call latency.

## Risk controls

- All `home.rs` isolation invariants preserved (throwaway `CODEX_HOME`, env allowlist,
  0600 auth, symlink rejection, no skills/MCP/hooks). Pool shares ONE isolated home;
  never the user's real `~/.codex`.
- Pool death/hang: per-turn `completion_timeout` retained; a timed-out child is killed
  and removed from the pool, not reused.
- Backward compatible: facade signature unchanged; Gemini/OpenAI paths untouched.

## Deferred (unchanged)

- Daemon / `unix://` / `ws://` transport — only if a future *agentic* (tool-enabled)
  Codex command needs it; not for synthesis.
- Provider profiles / `axon config provider` UX.
- `personality`, `serviceTier`, image input modality.
