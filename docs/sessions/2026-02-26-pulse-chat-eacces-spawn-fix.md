# Session: Pulse Chat EACCES Spawn Fix + Docker Env Consolidation
Date: 2026-02-26
Branch: feat/crawl-download-pack
Commit: ccbccfd

---

## Session Overview

Debugged and resolved a 502 Bad Gateway on Pulse chat (`POST /api/pulse/chat`) caused by three
layered failures in the Docker stack. Also consolidated env var placement per user's
single-source-of-truth policy (`.env` only, no inline docker-compose env vars). Path-traversal
hardening in `crates/web/download.rs` was part of the staged changes. Pre-commit fixes carried
from the previous session (rustfmt + clippy on refresh module) were committed and pushed
as `6f8f7c7` before this session's work began.

---

## Timeline

| Time | Activity |
|------|---------|
| Session start | Carried pre-commit fix: `cargo fmt` on `processor.rs:346` (line too long), confirmed all three `_with_pool` re-exports in `refresh/mod.rs` are used by `crates/cli/commands/refresh/schedule.rs:9-11`. Committed as `6f8f7c7`. |
| Phase 1 | Navigated to axon.tootie.tv via Chrome DevTools. Sent "hello, what model are you?" to Pulse chat. Got 502 Bad Gateway from Cloudflare on `POST /api/pulse/chat`. |
| Phase 2 | Root cause layer 1: `axon-web` container was running a stale image (built at 18:27, before claude install was added). Fixed by `docker compose up -d --build axon-web`. |
| Phase 3 | Root cause layer 2: `axon serve` was binding to `127.0.0.1:49000` (loopback only). `axon-web` on Docker bridge sends to `172.18.0.7:49000` → ECONNREFUSED. Root cause: `crates/web.rs:57` defaults to `127.0.0.1`. Fix: `AXON_SERVE_HOST=0.0.0.0`. |
| Phase 4 | User requested env var consolidation: "ALL env vars live SOLELY in the .env UNLESS necessary for an override." Moved `AXON_SERVE_HOST` from docker-compose `environment:` block to `.env` and `.env.example`. |
| Phase 5 | Root cause layer 3: `spawn claude EACCES`. `/usr/local/bin/claude` was a symlink → `/root/.local/share/claude/versions/2.1.61`. `/root/` has 700 perms; `node` user can't traverse through it in Node.js `spawn()`. Fixed Dockerfile: `cp "$(readlink -f /root/.local/bin/claude)" /usr/local/bin/claude`. |
| Verification | `curl POST /api/pulse/chat` → `{"text":"I'm Pulse, your document copilot — built on Anthropic's Claude (Sonnet)..."}` ✅ |
| Commit | `ccbccfd` pushed to `feat/crawl-download-pack`. |

---

## Key Findings

- **Symlink traversal failure** (`docker/web/Dockerfile`): The claude installer creates a symlink in `/root/.local/bin/`. Moving the symlink (not the binary) to `/usr/local/bin/claude` preserves the symlink. At runtime, Node.js `spawn()` as `node` user tries to dereference the symlink through `/root/.local/` which has 700 permissions — access denied. `docker exec -u node` worked because exec uses a different resolution path.
- **`crates/web.rs:57`**: `let host = std::env::var("AXON_SERVE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());` — loopback default breaks Docker bridge networking. Requires explicit `0.0.0.0` in container environments.
- **`/proc/net/tcp` hex decode**: `0100007F:BF68` = `127.0.0.1:49000` (LISTEN, broken); `00000000:BF68` = `0.0.0.0:49000` (LISTEN, fixed).
- **Docker compose yaml anchor `*common-service`** loads `.env` for all services — single source of truth. Any env var in the anchor's scope doesn't need to be duplicated inline.
- **`download.rs` path traversal**: Manifest-driven file serving was using raw `join(rel_path)` without validation. Added `is_safe_relative_manifest_path()` (rejects absolute paths, `..` components, null bytes) + `tokio::fs::canonicalize()`-based containment check.

---

## Technical Decisions

- **`cp "$(readlink -f ...)"` not `mv`**: `readlink -f` resolves the full chain of symlinks to the real binary. `cp` writes a new regular file at the destination, world-executable. `mv` would have moved just the symlink object, preserving the broken traversal issue.
- **`AXON_SERVE_HOST` in `.env` not docker-compose**: User policy: docker-compose yaml anchor already loads `.env` for all services — duplicating env vars inline creates maintenance burden and inconsistency. Only override inline when a service needs a different value from the `.env` default.
- **`chmod 755` after `cp`**: `cp` preserves source permissions. The installer may set the binary executable only for root. Explicit chmod ensures `node` user can execute it.
- **`is_safe_relative_manifest_path()` + `canonicalize()`**: Defense-in-depth. The path filter catches obvious attacks before filesystem access; canonicalize catches symlink-based escapes.

---

## Files Modified

| File | Purpose |
|------|---------|
| `docker/web/Dockerfile` | Fix EACCES: dereference symlink with `readlink -f` before `cp`; add `chmod 755` |
| `.env` | Add `AXON_SERVE_HOST=0.0.0.0` (moved from docker-compose inline env) |
| `.env.example` | Add documented `AXON_SERVE_HOST=0.0.0.0` entry |
| `docker-compose.yaml` | Remove `AXON_SERVE_HOST: "0.0.0.0"` from axon-workers inline environment block |
| `crates/web/download.rs` | Add `is_safe_relative_manifest_path()` + canonicalize-based path traversal prevention |
| `crates/web/execute/mod.rs` | WS execution bridge cleanup |
| `crates/web/execute/polling.rs` | Polling logic cleanup |
| `crates/web/execute/events.rs` | Event type cleanup |
| `crates/web/execute/files.rs` | File serving cleanup |
| `crates/web/execute/tests/ws_event_v2_tests.rs` | Test alignment |
| `crates/web/docker_stats.rs` | Stats streaming cleanup |
| `CHANGELOG.md` | Updated TBD → `6f8f7c7` and added `ccbccfd` entry |

---

## Commands Executed

```bash
# Rebuild stale axon-web image
docker compose up -d --build axon-web

# Verify claude binary after fix
docker compose exec -u node axon-web /usr/local/bin/claude --version
# → 2.1.61 (Claude Code)

# Verify chat works end-to-end
curl -s -X POST http://localhost:49010/api/pulse/chat \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"hello"}]}' | jq .text
# → "I'm Pulse, your document copilot — built on Anthropic's Claude (Sonnet)..."

# Decode /proc/net/tcp to confirm bind address changed
cat /proc/net/tcp | awk '$2 ~ /BF68$/ {print $2, $4}'
# Before fix: 0100007F:BF68  0A (LISTEN) → 127.0.0.1:49000
# After fix:  00000000:BF68  0A (LISTEN) → 0.0.0.0:49000

# Commit and push
git add . && git commit -m "fix(docker+web): dereference claude symlink ..."
git push
# → 6f8f7c7..ccbccfd feat/crawl-download-pack → feat/crawl-download-pack
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `POST /api/pulse/chat` | 502 Bad Gateway (Cloudflare) | 200 OK with Claude response |
| `spawn('claude', ...)` in `axon-web` | `EACCES` — symlink traversal through `/root/.local/` (700) | Executes successfully — real binary at `/usr/local/bin/claude` |
| `axon serve` bind | `127.0.0.1:49000` (loopback only) | `0.0.0.0:49000` (all interfaces) |
| `docker-compose.yaml` env | `AXON_SERVE_HOST` duplicated inline | All env vars in `.env` only |
| `download.rs` manifest paths | Raw `join(rel_path)` — path traversal possible | `canonicalize()` + component filter |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `docker compose exec -u node axon-web /usr/local/bin/claude --version` | `2.1.61 (Claude Code)` | `2.1.61 (Claude Code)` | ✅ |
| `curl POST /api/pulse/chat` | 200 + text field | `{"text":"I'm Pulse, your document copilot..."}` | ✅ |
| `git push` | `6f8f7c7..ccbccfd` | `6f8f7c7..ccbccfd feat/crawl-download-pack` | ✅ |
| `cargo clippy` (pre-commit) | 0 warnings | 0 warnings | ✅ |
| `cargo fmt --check` (pre-commit) | clean | clean | ✅ |

---

## Source IDs + Collections Touched

| Action | Source ID | Collection | Outcome |
|--------|-----------|------------|---------|
| embed session doc | TBD (post-embed) | axon_rust | TBD |

---

## Risks and Rollback

- **Dockerfile change is build-time**: `docker compose build axon-web` required to apply. Running containers are unaffected until rebuild. To rollback: revert `docker/web/Dockerfile` and rebuild.
- **`AXON_SERVE_HOST` in `.env`**: If `.env` is missing or misconfigured, `axon serve` falls back to `127.0.0.1` (loopback) and `axon-web` can't reach workers. Symptom: ECONNREFUSED on all API routes. Fix: ensure `.env` has `AXON_SERVE_HOST=0.0.0.0`.
- **Path traversal hardening** is additive/non-breaking: invalid paths are skipped rather than erroring the whole download. Legitimate manifest entries are unaffected.

---

## Decisions Not Taken

- **Add `/var/run/docker.sock` mount to `axon-web`**: Considered but rejected — the `axon-web` container only needs to run CLIs and proxy to `axon-workers`, not introspect Docker. The Docker stats feature lives in `axon-workers` (bollard).
- **Symlink in `/root/.local/` vs just copying binary**: Could have installed claude as root and pointed symlink elsewhere, but the cleanest fix is to own a real binary at `/usr/local/bin/claude` with world-executable perms.
- **Keep `AXON_SERVE_HOST` in docker-compose for visibility**: Rejected per user's explicit policy: all env vars in `.env` unless a per-service override is needed.

---

## Open Questions

- **WS proxy ECONNREFUSED** (`Failed to proxy http://axon-workers:49000/ws`): WebSocket path in axon-web Next.js proxy still shows ECONNREFUSED in logs. Chat API works (HTTP), but WS streaming may be broken. Needs investigation.
- **`pulse` Qdrant collection missing**: `[Pulse] Qdrant upsert failed: Collection 'pulse' doesn't exist`. The Pulse save/RAG route is trying to write to a `pulse` collection that hasn't been created. Needs `ensure_collection()` call or collection pre-creation.
- **`docker exec -u node` vs Node.js `spawn()` path resolution**: Why does `docker exec` as node successfully resolve the symlink but `spawn()` fails? The exact kernel/libc behavior difference was not fully investigated — the `readlink -f` fix sidesteps the question.

---

## Next Steps

1. Investigate WS proxy ECONNREFUSED for `axon-workers:49000/ws` — check Next.js `next.config.js` proxy config and whether `axon serve` serves WS on port 49000.
2. Create `pulse` Qdrant collection (or add `ensure_collection("pulse")` call in the Pulse save route).
3. Update CHANGELOG.md TBD → `ccbccfd` (done in this session after push).
4. Rebuild `axon-web` in production environment to deploy the Dockerfile fix.
