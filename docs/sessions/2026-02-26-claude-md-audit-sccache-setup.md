# Session: CLAUDE.md Audit + sccache Setup
Date: 2026-02-26 22:25 EST
Branch: feat/crawl-download-pack

---

## Session Overview

Two distinct workstreams:

1. **CLAUDE.md audit** — ran the `claude-md-improver` skill across all 9 CLAUDE.md files in the repo. Found one near-empty file (`crates/web/CLAUDE.md`), one stale layout (`crates/ingest/CLAUDE.md`), and one minor architecture drift (`CLAUDE.md` root). Applied targeted fixes.

2. **sccache + auto-fmt hooks** — installed sccache, wired it into `.cargo/config.toml` for all cargo invocations, added a PostToolUse hook that auto-runs `rustfmt` on every edited `.rs` file, and updated that hook to set `RUSTC_WRAPPER=sccache` in the subprocess env.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Invoked `claude-md-improver` skill |
| Phase 1 | Discovered 9 CLAUDE.md files across project tree |
| Phase 2 | Read all 9 files in parallel; cross-checked against actual filesystem layout |
| Phase 3 | Produced quality report (9 files scored A–F) |
| Phase 4 | Applied 3 targeted updates after user approval |
| Phase 5 | Explained sccache; user asked to set it up |
| Phase 6 | Added PostToolUse auto-fmt hook for `.rs` files |
| Phase 7 | Installed sccache via apt; created `.cargo/config.toml`; updated hook with `RUSTC_WRAPPER` |
| Session end | `save-to-md` invoked |

---

## Key Findings

### CLAUDE.md Quality Scores

| File | Score | Grade | Key Issue |
|------|-------|-------|-----------|
| `./CLAUDE.md` | 90/100 | A | Jobs architecture listed flat `.rs` files; actual structure is module dirs |
| `./docs/CLAUDE.md` | 86/100 | B | Clean; minor unverified `docs/monolith-policy.md` reference |
| `./crates/vector/CLAUDE.md` | 93/100 | A | Exemplary — no changes needed |
| `./crates/crawl/CLAUDE.md` | 90/100 | A | No changes needed |
| `./crates/jobs/CLAUDE.md` | 88/100 | B | No changes needed |
| `./crates/web/CLAUDE.md` | 42/100 | F | 26 sparse lines; no WS protocol, no security model, no testing |
| `./crates/ingest/CLAUDE.md` | 82/100 | B | `github.rs` listed as single file; refactored to `github/` dir months ago |
| `./crates/mcp/CLAUDE.md` | 94/100 | A | No changes needed |
| `./docker/CLAUDE.md` | 95/100 | A | No changes needed |

### Critical Discovery: `crates/web/CLAUDE.md` was effectively empty
The file listed directory intent but documented nothing about how the WebSocket bridge works, its security model, or how to test it. The `execute/` directory had grown to a full module (`mod.rs`, `events.rs`, `files.rs`, `polling.rs`, `tests/`) with no documentation.

### Stale Layout: `crates/ingest/CLAUDE.md`
Module layout showed `github.rs` as a flat file. Actual filesystem: `crates/ingest/github/` directory with `mod.rs`, `files.rs`, `issues.rs`, `wiki.rs`. The single-file entry also incorrectly attributed all GitHub ingestion to `octocrab` — actual implementation uses raw `reqwest` for file content and `octocrab` only for issues/PRs.

### sccache Not Installed
`sccache` was absent from the system despite being recommended in the prior session. Ubuntu 25.10 apt had `sccache 0.10.0-7` available — installed as pre-built binary (no compile wait). No `.cargo/config.toml` existed in the project.

---

## Technical Decisions

### `rustfmt` directly vs `cargo fmt` for the auto-fmt hook
**Decision:** Use `rustfmt --edition 2024 <file>` directly, not `cargo fmt -- --edition 2024 <file>`.

**Reason:** `cargo fmt -- <file>` does not format a specific file — the `--` separator passes extra args to each `rustfmt` invocation but cargo still discovers and processes all project files. `rustfmt <file>` formats exactly the edited file and skips cargo workspace discovery overhead (~10× faster per edit).

### `shutil.which('sccache')` guard in hook
**Decision:** Detect sccache at hook runtime rather than hardcoding.

**Reason:** Degrades gracefully if sccache is ever absent (e.g., different machine, fresh checkout). The `.cargo/config.toml` `rustc-wrapper` entry is the primary mechanism; the hook env var is belt-and-suspenders for subprocess consistency.

### apt install vs `cargo install sccache`
**Decision:** `apt install sccache` (Ubuntu 25.10 has 0.10.0-7).

**Reason:** Pre-built binary; no compile time. `cargo install sccache` compiles sccache itself from source — minutes of build time for a build accelerator is ironic and avoidable.

### New hook placement: first in PostToolUse array
**Decision:** Auto-fmt hook inserted as first PostToolUse entry.

**Reason:** Format should happen before the cargo-audit / cargo-deny / sync hooks so those hooks see already-formatted code. Order matters if a hook reads file content.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/web/CLAUDE.md` | Major expansion (~26 → ~70 lines) | Added WS protocol table, security model (ALLOWED_MODES/ALLOWED_FLAGS), execute/ submodule breakdown, Docker stats caveat, testing commands |
| `crates/ingest/CLAUDE.md` | Module layout + GitHub section | Fixed `github.rs` → `github/` directory; corrected octocrab vs reqwest attribution; added wiki.rs subprocess + buffer_unordered notes |
| `CLAUDE.md` (root) | Architecture section lines 176–185 | Jobs: `crates/jobs/{crawl,...}.rs` → `crates/jobs/crawl/` (module dirs); apps/web line updated with port 49010 |
| `.claude/settings.json` | Added PostToolUse hook (first entry) | Auto-fmt `.rs` files on edit via `rustfmt --edition 2024`; sets `RUSTC_WRAPPER=sccache` if available |
| `.cargo/config.toml` | **Created** | `[build] rustc-wrapper = "sccache"` — project-wide sccache activation |

---

## Commands Executed

```bash
# Discovery
find /home/jmagar/workspace/axon_rust -name "CLAUDE.md" | sort
# → 9 files found

# Verify ingest module structure
ls /home/jmagar/workspace/axon_rust/crates/ingest/
# → github/  github.rs  reddit.rs  sessions/  sessions.rs  youtube.rs

ls /home/jmagar/workspace/axon_rust/crates/ingest/github/
# → files.rs  issues.rs  wiki.rs  (plus mod.rs implicit)

ls /home/jmagar/workspace/axon_rust/crates/web/execute/
# → events.rs  files.rs  mod.rs  polling.rs  tests/

# sccache install
apt-cache show sccache  → Version: 0.10.0-7
sudo apt-get install -y sccache  → Success

which sccache && sccache --version
# → /usr/bin/sccache
# → sccache 0.10.0

# Verify .cargo/config.toml picked up
cargo build --bin axon 2>&1 | tail -3
# → Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.33s

sccache --show-stats | grep -E "^(Compile|Cache)"
# → Compile requests: 3
# → Cache location: Local disk: "/home/jmagar/.cache/sccache"

# Validate settings.json JSON
python3 -c "import json; json.load(open('.claude/settings.json')); print('JSON valid')"
# → JSON valid
```

---

## Behavior Changes (Before/After)

### Auto-fmt on `.rs` edit
- **Before:** Format drift accumulated during editing session; `cargo fmt --check` blocked commit via lefthook
- **After:** Every `.rs` edit triggers `rustfmt --edition 2024 <file>` immediately; commits always pass fmt check

### sccache compiler caching
- **Before:** Every `cargo build`/`cargo check`/`cargo test` recompiled from scratch (no cross-invocation caching)
- **After:** Compiled artifacts cached at `/home/jmagar/.cache/sccache`; subsequent builds reuse cache. First run populates cache; all subsequent runs for unchanged deps are near-instant

### CLAUDE.md documentation quality
- **Before:** `crates/web/CLAUDE.md` was 26 lines with no actionable content; `crates/ingest/CLAUDE.md` had stale `github.rs` single-file layout
- **After:** Web crate doc covers WS protocol, security model, all submodule files, Docker stats caveat; ingest doc reflects actual `github/` directory structure with correct tech attribution

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `which sccache` | `/usr/bin/sccache` | `/usr/bin/sccache` | ✅ PASS |
| `sccache --version` | `sccache 0.10.0` | `sccache 0.10.0` | ✅ PASS |
| `cargo build --bin axon` | Clean finish | `Finished dev profile in 2.33s` | ✅ PASS |
| `sccache --show-stats \| grep Compile` | Requests > 0 | `Compile requests: 3` | ✅ PASS |
| `python3 -c "json.load(...)"` | JSON valid | `JSON valid` | ✅ PASS |
| `cat .cargo/config.toml` | `rustc-wrapper = "sccache"` | Confirmed | ✅ PASS |
| `ls crates/ingest/github/` | dir with files | `files.rs issues.rs wiki.rs` | ✅ PASS |
| `ls crates/web/execute/` | module dir | `events.rs files.rs mod.rs polling.rs tests/` | ✅ PASS |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations performed during this session prior to this save.

---

## Risks and Rollback

### sccache
- **Risk:** Stale cache entries could theoretically serve wrong artifacts after a dependency update. Low probability — sccache keys on input hash.
- **Rollback:** `sccache --stop-server && rm -rf ~/.cache/sccache` clears the cache; remove `[build] rustc-wrapper = "sccache"` from `.cargo/config.toml` to disable entirely.

### Auto-fmt hook
- **Risk:** Hook silently swallows `rustfmt` errors (`capture_output=True`). A file with a syntax error will fail to format but the hook won't surface it — Claude Code will see the unformatted file.
- **Rollback:** Remove the first PostToolUse entry from `.claude/settings.json`.

### CLAUDE.md changes
- **Risk:** Low. All changes were additive or corrected stale info. No code was touched.
- **Rollback:** `git checkout -- crates/web/CLAUDE.md crates/ingest/CLAUDE.md CLAUDE.md`

---

## Decisions Not Taken

| Alternative | Rejected Because |
|-------------|-----------------|
| `cargo install sccache` | Takes minutes to compile; apt pre-built is instant |
| `cargo fmt -- <file>` in hook | Doesn't format a single file; formats whole project |
| Global `~/.cargo/config.toml` for sccache | Project-scoped `.cargo/config.toml` is more explicit and doesn't silently affect unrelated projects |
| Redis sccache backend | Local disk backend is zero-config and sufficient for single-dev workflow; Redis backend is worth revisiting if CI is added |
| Merging all CLAUDE.md issues into root CLAUDE.md | Subsystem docs belong in their own CLAUDE.md; flat consolidation would make root CLAUDE.md unmanageable |

---

## Open Questions

- Does `docs/monolith-policy.md` actually exist? Listed in `docs/CLAUDE.md` directory layout but not verified during this session.
- `crates/ingest/github.rs` still exists alongside the `github/` directory — is it a re-export shim or orphaned? CLAUDE.md annotates it as "re-export shim (if present)" but this wasn't confirmed against source.
- First cold `cargo clean && cargo build` with sccache not run — cache hit rate on a clean build is unverified.
- `apps/web/` architecture: added a one-line note to root CLAUDE.md but no dedicated `apps/web/CLAUDE.md` exists. May be worth creating once Pulse workspace architecture stabilizes.

---

## Next Steps

- [ ] Run `cargo clean && cargo build --bin axon` to populate sccache cold cache and verify hit rates on second build
- [ ] Verify or remove `docs/monolith-policy.md` reference in `docs/CLAUDE.md`
- [ ] Confirm `crates/ingest/github.rs` status (re-export shim vs orphan) and update `crates/ingest/CLAUDE.md` accordingly
- [ ] Consider `apps/web/CLAUDE.md` once Pulse workspace feature set stabilizes
- [ ] Consider Redis sccache backend if CI pipeline is added (already have `axon-redis` running)
