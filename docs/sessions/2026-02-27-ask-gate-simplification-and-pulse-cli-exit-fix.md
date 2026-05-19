# Session: ask Gate Simplification + Pulse CLI Exit Fix

**Date:** 2026-02-27
**Branch:** feat/crawl-download-pack
**Commits:** d7ad5bb, 5066461, 884af14

---

## Session Overview

Two bugs diagnosed and fixed:

1. **`axon ask` returning "Insufficient evidence"** on legitimate queries — Gates 5 & 6 used brittle URL heuristics (`/guide/` singular) that false-positived on real documentation URLs like `/guides/`. Fix: removed Gates 5 & 6 entirely and all associated dead code.

2. **Pulse chat "Claude CLI exited 1"** — `${AXON_DATA_DIR}/axon/claude` bind-mount directories (`todos/`, `projects/`, `debug/`, etc.) were root-owned. Claude CLI running as the `node` user (UID 1000) couldn't write session state → `EACCES` → exit code 1. Fix: new cont-init script that `chown -R node:node` on every container start.

---

## Timeline

1. **Check for prior commits/plans** — Found no existing commits removing gates; no plans present. Found instead two recent commits *adding* gates (234989b, 7be0ba0).
2. **Code review** — Read `ask.rs`, `context.rs`, `ranking.rs`, `streaming.rs`, `config/types.rs`, `config/parse.rs`.
3. **Live test via MCP** — Ran `axon ask` via MCP tool, received "Insufficient evidence" block on spider.rs question.
4. **Root cause diagnosed** — `url_path_is_docs_like()` checked `path.starts_with("guide/")` (singular); actual URL path was `/guides/` (plural) → Gate 5 false-positive.
5. **Wrote implementation plan** — `docs/plans/2026-02-27-ask-gate-simplification.md`.
6. **Implemented Tasks 1–3** (subagent): removed 7 dead functions, simplified `normalize_ask_answer`, removed config fields.
7. **Spec review** — COMPLIANT (one stale comment noted).
8. **Code quality review** — Found stale comment + DRY violation (duplicate domain-matching logic).
9. **Fixed quality issues** — Removed stale comment, inlined `host_from_source` into `source_matches_domain_list`.
10. **Clippy** caught `env_bool` in `performance.rs` now unused → deleted it.
11. **All green**: 442 tests, 0 clippy warnings, fmt clean.
12. **Smoke test confirmed**: `axon ask "how does spider.rs handle JavaScript-heavy sites?"` now returns real answer citing `spider.cloud/guides/spider/`.
13. **Committed** `d7ad5bb` + changelog `5066461`.
14. **New bug reported** — Pulse chat screenshot showing "Claude CLI exited 1".
15. **Systematic debug** — Located spawn code in `apps/web/app/api/pulse/chat/route.ts:181`.
16. **Reproduced error** — `docker exec -u node ... claude --dangerously-skip-permissions` → `EACCES: permission denied, open '/home/node/.claude/todos/...'`.
17. **Root cause confirmed** — `todos/`, `projects/`, `debug/`, `cache/` all owned by `root` on host `/home/jmagar/appdata/axon/claude/`.
18. **Fixed**: new cont-init script + immediate `sudo chown -R jmagar:jmagar` on host.
19. **Verified**: same claude invocation now streams successfully.
20. **Committed** `884af14`.

---

## Key Findings

### ask Gate Simplification

- **`url_path_is_docs_like()` bug** (`ask.rs`, now deleted): checked `path.starts_with("guide/")` — singular. Real spider.rs docs are at `/guides/` — plural. Gate 5 was firing on valid answers.
- **Gate 5 (`ask_strict_procedural`)** and **Gate 6 (`ask_strict_config_schema`)** both used the same URL-heuristic approach. Both produced false positives. The LLM system prompt already enforces citation grounding — Gates 1–4 are sufficient.
- **`performance::env_bool`** (`crates/core/config/parse/performance.rs`) — became fully unused after config field deletion. Clippy caught it with `-D warnings`.
- **DRY violation**: `ask.rs` had a copy of `host_from_source` + `source_matches_domain_list` logic that duplicated the same logic in `context.rs`. Fixed by inlining.
- **`source_matches_domain_list` still needed** — Gate 4b (authoritative allowlist, opt-in, empty by default) uses it. Only the functions specific to Gates 5/6 were dead.

### Pulse "Claude CLI exited 1"

- **Spawn location**: `apps/web/app/api/pulse/chat/route.ts:181` — `spawn('claude', args, { cwd: os.tmpdir(), env: childEnv, stdio: ['ignore', 'pipe', 'pipe'] })`
- **Claude args**: built in `claude-stream-types.ts:60` — uses `--mcp-config /home/node/.claude/mcp.json --strict-mcp-config --dangerously-skip-permissions`
- **Error mechanism**: Claude CLI attempts to write `/home/node/.claude/todos/<session-id>.json` on startup. `EACCES: permission denied` → `error_during_execution` result event → exit code 1 → `route.ts:310` emits `"Claude CLI exited ${code}: ..."`.
- **Ownership cause**: Directories created as root during previous `docker exec` sessions or earlier container runs. Bind-mount doesn't reset ownership on restart.
- **`node` user**: Next.js server runs as UID 1000 via `s6-setuidgid node` in `docker/web/s6-rc.d/pnpm-dev/run`. Spawned subprocesses inherit this UID.
- **Volume path**: `${AXON_DATA_DIR:-./data}/axon/claude:/home/node/.claude` (docker-compose.yaml:221).

---

## Technical Decisions

### Why remove Gates 5 & 6 entirely (not fix the heuristic)

The heuristic approach is inherently fragile — any non-standard URL path pattern can break it. Options considered:
- **Option A**: Fix `/guide/` → `/guides/` bug. Rejected: whack-a-mole. Other path patterns will fail too.
- **Option B (chosen)**: Remove Gates 5 & 6 entirely. The LLM system prompt already enforces citation grounding. Gates 1–4 catch: no citations, LLM self-flagged, unmapped citations, min-citation count. No quality regression expected.
- **Option C**: Make gates configurable (default off). Rejected: adds config complexity for features being removed.

### Why cont-init for the ownership fix (not Dockerfile)

- Dockerfile `RUN chown` would only work at build time — volume mounts override at runtime.
- `docker-compose.yaml` bind-mount can't set ownership.
- cont-init runs as root on every container start, before s6 drops to `node` for services — correct timing to fix ownership before the Node.js server spawns claude.
- Alternative: fix on host via cron/oneshot. Rejected: cont-init is self-contained in the repo.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/vector/ops/commands/ask.rs` | Removed `AskQueryClass`, `classify_query`, `is_official_docs_source`, `url_path_is_docs_like`, `host_from_source`, `query_file_like_tokens`, `has_exact_page_citation`; simplified `normalize_ask_answer`; removed stale comment; inlined `host_from_source` into `source_matches_domain_list`; deleted 7 tests |
| `crates/core/config/types.rs` | Removed `ask_strict_procedural`, `ask_strict_config_schema` fields, defaults, Debug impl, test assertions |
| `crates/core/config/parse.rs` | Removed 2 `env_bool` parse lines for deleted config fields |
| `crates/core/config/parse/performance.rs` | Deleted `pub(super) fn env_bool()` — became unused |
| `docs/commands/ask.md` | Updated Notes section removing Gate 5/6 references |
| `.env.example` | Removed `AXON_ASK_STRICT_PROCEDURAL` and `AXON_ASK_STRICT_CONFIG_SCHEMA` entries |
| `CHANGELOG.md` | Added entry for `d7ad5bb` |
| `docs/plans/2026-02-27-ask-gate-simplification.md` | Implementation plan (created) |
| `docker/web/cont-init.d/15-fix-claude-dir-ownership` | New cont-init: `chown -R node:node /home/node/.claude` on every container start |

---

## Commands Executed

```bash
# Diagnose ask false-positive
axon ask "how does spider.rs handle JavaScript-heavy sites?" --diagnostics

# Verify fix
cargo check 2>&1 | grep error
cargo test --lib 2>&1 | tail -5   # 442 tests, 0 failures
cargo clippy -- -D warnings        # 0 warnings
cargo fmt --check                  # clean

# Smoke test
./scripts/axon ask "how does spider.rs handle JavaScript-heavy sites?" --diagnostics

# Reproduce Pulse CLI exit 1
docker exec -u node -e CLAUDECODE="" axon-web claude -p "what up" \
  --output-format stream-json --verbose --system-prompt "You are helpful." \
  --mcp-config /home/node/.claude/mcp.json --strict-mcp-config \
  --dangerously-skip-permissions --include-partial-messages \
  --effort medium --model sonnet 2>&1 | head -10
# Result: EACCES: permission denied, open '/home/node/.claude/todos/...'

# Fix host-side ownership immediately
sudo chown -R jmagar:jmagar /home/jmagar/appdata/axon/claude/

# Verify fix
docker exec -u node -e CLAUDECODE="" axon-web claude -p "what up" \
  [same args as above] 2>&1 | head -3
# Result: streaming system init event + message_start + thinking delta
```

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| `axon ask "how does spider.rs handle JavaScript-heavy sites?"` | "Insufficient evidence" block — Gate 5 fired on `spider.cloud/guides/spider/` | Full answer with `## Sources` citing the guides URL |
| Pulse chat "what up" | "Claude CLI exited 1:" error message | Streams a real response |
| `ask` on any query where best source has `/guides/`, `/tutorials/`, or non-standard path prefix | Gate 5 false-positive → "Insufficient evidence" | Passes Gates 1–4; answer returned |
| `ask` with truly no relevant sources (LLM says "I don't know") | Gate 2 triggers correctly | Gate 2 still triggers correctly (unchanged) |
| Container restart | `.claude` subdirs may be root-owned → Pulse broken | cont-init fixes ownership on start |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test --lib \| tail -5` | 0 failures | `test result: ok. 442 passed; 0 failed` | ✅ |
| `cargo clippy -- -D warnings` | 0 warnings | 0 warnings | ✅ |
| `cargo fmt --check` | clean | clean | ✅ |
| `grep -r "ask_strict_procedural\|AXON_ASK_STRICT" . --include="*.rs"` | 0 results | 0 results | ✅ |
| `axon ask "how does spider.rs..." --diagnostics` | Answer with spider.cloud citation | Full answer + `spider.cloud/guides/spider/` cited | ✅ |
| `docker exec -u node claude -p "what up" ...` | Streaming JSON | `{"type":"system",...}` init event + tokens streaming | ✅ |
| `ls -la /home/jmagar/appdata/axon/claude/todos/` | node-owned | `jmagar:jmagar` (UID 1000 in container) | ✅ |

---

## Source IDs + Collections Touched

- No Qdrant embed/retrieve operations in this session.
- MCP `axon ask` calls hit the `cortex` collection (read-only, no modification).

---

## Risks and Rollback

### ask Gate Simplification

- **Risk**: Answers that relied on Gate 5/6 to block low-quality non-docs sources will now pass through. Assessed as low risk — the LLM system prompt instructs citation grounding; Gates 1–4 catch the failure cases that don't require URL heuristics.
- **Rollback**: `git revert d7ad5bb` — restores all deleted functions and config fields. Config test assertions must also be re-added.

### Pulse CLI Exit Fix

- **Risk**: `chown -R node:node` on a bind-mount also changes `.credentials.json` and `.claude.json.backup.*` ownership. These files were root-owned; changing them to node-owned is harmless since they need to be readable/writable by the claude process (which runs as node).
- **Rollback**: Remove `docker/web/cont-init.d/15-fix-claude-dir-ownership`. Re-root-own the dirs with `sudo chown -R root:root /home/jmagar/appdata/axon/claude/todos /home/jmagar/appdata/axon/claude/projects ...` (not recommended — restores the bug).

---

## Decisions Not Taken

- **Option A for ask fix** (patch `/guide/` → `/guides/`): Whack-a-mole. Pattern would still fail on `/tutorial/`, `/documentation/`, `/manual/`, etc.
- **Option C for ask fix** (make gates configurable, default off): Adds env-var surface area for features being removed. YAGNI.
- **Dockerfile approach for ownership fix**: `RUN chown` at build time is overridden by volume mounts at runtime. Doesn't work.
- **Host-only fix for ownership** (cron or manual `chown`): Not self-contained — breaks on first `docker compose down && up` in a fresh environment.
- **Using `--allow-root` or similar Claude CLI flag**: No such flag exists.

---

## Open Questions

- **Why were the dirs created as root in the first place?** Most likely: a prior `docker exec` session as root (default exec user) triggered Claude CLI to initialize the `.claude` dir structure as root. The cont-init fix prevents this going forward, but the exact original trigger is not confirmed.
- **`chrome-devtools` and `neo4j-memory` MCP servers show `"status":"failed"`** in the Claude init event inside the container — expected (chrome not reachable from container network on this run, neo4j-memory disabled). Not a bug.

---

## Next Steps

- Rebuild the `axon-web` image to bake the new cont-init script: `docker compose build axon-web && docker compose up -d axon-web`
- Verify Pulse chat works end-to-end after container rebuild (not just `docker exec` test).
- Consider adding a health check or startup log assertion that verifies `.claude` dirs are node-writable before the pnpm-dev service starts.
