# Fix CI: cargo-audit flag + Qdrant keyword indexes

**Date**: 2026-03-02
**Branch**: feat/sidebar
**PR**: #6 (https://github.com/jmagar/axon_rust/pull/6)
**Commit**: `9428156c`

---

## Session Overview

Diagnosed and fixed two pre-existing CI failures on the `feat/sidebar` PR — both also failing on `main` across all 3 recent runs. Neither failure was introduced by this branch's changes. Fixed in a single commit with zero test regressions (589 tests pass, clippy clean).

---

## Timeline

1. Invoked `/gh-fix-ci` skill
2. Resolved PR #6 on `feat/sidebar` (`gh pr view --json number,url`)
3. Unset invalid `GITHUB_TOKEN` env var that was blocking `gh` auth
4. Fetched failing check names: `security` and `mcp-smoke`
5. Pulled CI logs for both jobs via `gh run view --log --job <id>`
6. Downloaded `mcp-smoke-logs` artifact to `/tmp/mcp-smoke-logs/`
7. Read `summary.txt`: PASS=22 FAIL=2 (`action_domains`, `action_sources`)
8. Read test script `scripts/test-mcp-tools-mcporter.sh` — confirmed tests check `.ok == true and .action == "domains"/"sources"`
9. Traced `handle_domains`/`handle_sources` → `domains_payload`/`sources_payload` → `qdrant_domain_facets`/`qdrant_url_facets`
10. Found that test helper `create_keyword_index` comments say "required for /facet" — confirmed root cause
11. Confirmed `ensure_collection` never creates keyword indexes — only creates vector collection
12. Added `ensure_payload_indexes` to `qdrant_store.rs` and called it from `ensure_collection`
13. Fixed `cargo audit --deny vulnerability` → `cargo audit` in `.github/workflows/ci.yml`
14. Verified with `cargo check`, `cargo clippy`, all 589 tests pass via pre-commit hooks
15. Pushed to remote

---

## Key Findings

- **`security` failure root cause**: `cargo audit v0.22.1` removed `vulnerability` as a valid `--deny` argument. Error: `"invalid deny option: vulnerability"`. The binary exits non-zero on vulns by default — `--deny vulnerability` is redundant and now invalid.
- **`mcp-smoke` failure root cause**: `ensure_collection` in `crates/vector/ops/tei/qdrant_store.rs:21` creates the Qdrant collection and upserts vectors, but never creates keyword payload indexes on `url` and `domain` fields. The `/facet` endpoint (used by `qdrant_url_facets` and `qdrant_domain_facets`) requires these indexes.
- Both failures were present on `main` across all 3 recent runs — not caused by this PR's changes.
- The integration tests in `crates/vector/ops/qdrant/tests.rs:29` already documented this requirement: `"Helper: create a keyword payload index on the given field (required for /facet)"`.
- Dependabot reports 2 high-severity npm vulnerabilities (minimatch CVE-2026-27903, CVE-2026-27904) — these are JS/npm, not Rust; `cargo audit` won't surface them. Already patched in commit `149325f0`.

---

## Technical Decisions

- **Placed `ensure_payload_indexes` inside `ensure_collection`** rather than in the MCP handlers or facet functions. Rationale: collection setup is the right boundary — the same collection that gets vectors should also have the indexes required to query them efficiently. Any process that calls `ensure_collection` (embed, mcp, workers) will now guarantee indexes exist.
- **Called `ensure_payload_indexes` on both paths** in `ensure_collection`: the early-return path (collection already exists with correct dim) and the new-collection path. Rationale: existing production collections that predate this change also need indexes created idempotently.
- **Just `cargo audit`** without any `--deny` flags. Rationale: default exit code behavior is sufficient — exits non-zero on any known vulnerability.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/vector/ops/tei/qdrant_store.rs` | Added `ensure_payload_indexes` fn; called from `ensure_collection` on both code paths | Creates keyword indexes on `url`/`domain` fields required by Qdrant `/facet` endpoint |
| `.github/workflows/ci.yml` | `cargo audit --deny vulnerability` → `cargo audit` | Remove invalid flag that caused immediate exit code 2 in cargo-audit v0.22.1 |

---

## Commands Executed

```bash
# Diagnosed CI failures
unset GITHUB_TOKEN && gh pr view --json number,url
unset GITHUB_TOKEN && gh pr checks 6 --json name,state,bucket,link,startedAt,completedAt
unset GITHUB_TOKEN && gh run view 22586652456 --log --job 65433320556   # security
unset GITHUB_TOKEN && gh run view 22586652456 --log --job 65433320548   # mcp-smoke
unset GITHUB_TOKEN && gh run download 22586652456 --name mcp-smoke-logs --dir /tmp/mcp-smoke-logs

# Confirmed both failures pre-existed on main
unset GITHUB_TOKEN && gh run list --branch main --limit 3 --json databaseId,conclusion,name
unset GITHUB_TOKEN && gh run view 22544418932 --json jobs --jq '.jobs[] | select(.conclusion == "failure") | .name'
# → mcp-smoke, security (same jobs failing on main)

# Verified changes
cargo check --bin axon    # Finished in 0.96s
cargo clippy --bin axon   # 0 warnings

# Pre-commit hooks (run automatically on commit)
# 589 tests pass, 0 failures, 3 ignored

# Push
unset GITHUB_TOKEN && git push
```

---

## Behavior Changes (Before/After)

| Context | Before | After |
|---------|--------|-------|
| `cargo audit` CI step | Exits immediately with code 2: `"invalid deny option: vulnerability"` — never actually checks for vulns | Runs correctly; exits 0 if no Rust crate vulnerabilities, non-zero if found |
| `action:domains` MCP call | Qdrant returns HTTP 400 (no keyword index on `domain`) → MCP error response → jq `.ok == true` fails → FAIL | `ensure_collection` now creates keyword index on `domain` → `/facet` returns results → `.ok == true` → PASS |
| `action:sources` MCP call | Same as above but for `url` field | Same fix — keyword index on `url` created at collection init |
| Collection init (`ensure_collection`) | Creates vector collection only | Creates vector collection + keyword indexes on `url` and `domain` (idempotent) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | Finished | `Finished dev profile [unoptimized + debuginfo] target(s) in 0.96s` | ✅ PASS |
| `cargo clippy --bin axon` | 0 warnings | `Finished dev profile [unoptimized + debuginfo] target(s) in 31.90s` (0 warnings) | ✅ PASS |
| `cargo test --lib` (via pre-commit) | 589 pass | `589 passed; 0 failed; 3 ignored` | ✅ PASS |
| `wc -l crates/vector/ops/tei/qdrant_store.rs` | ≤500 lines | `129` | ✅ PASS |
| `gh run list --branch main --limit 3` | Same failures on main | `mcp-smoke` + `security` failing on all 3 main runs | ✅ Confirmed pre-existing |
| `git push` | Pushed | `feat/sidebar -> feat/sidebar` (remote accepted) | ✅ PASS |

---

## Source IDs + Collections Touched

None — this session was pure code/CI fix; no Axon embed/retrieve operations were performed during implementation.

---

## Risks and Rollback

**Risk**: `ensure_payload_indexes` adds 2 additional HTTP requests to Qdrant on every new collection init or first embed per process. These are fast (index creation is near-instant on small collections) and idempotent. Impact: negligible.

**Risk**: If Qdrant is unavailable when `ensure_collection` is called, the index creation will fail and bubble up as an error. This was already the case for the collection creation itself — no change in failure semantics.

**Rollback**: Revert commit `9428156c`. The `cargo audit` step would break again, and MCP `action:domains`/`action:sources` would return errors for fresh collections.

---

## Decisions Not Taken

- **Adding index creation to `qdrant_url_facets`/`qdrant_domain_facets` directly**: Would be self-healing per call but adds overhead on every facet query. Collection setup is the right boundary.
- **Adding a CI step to create indexes after seed embed**: Would fix CI but leave production deployments broken. The code-level fix is correct.
- **Using `--deny warnings` in cargo audit**: Broader than needed — would flag unmaintained crates and cause false positives. Default behavior is sufficient.
- **Fixing npm vulnerabilities in this session**: Out of scope; already patched in commit `149325f0` on this branch.

---

## Open Questions

- Will `cargo audit` find any Rust-specific vulnerabilities once the flag is fixed? The 2 GitHub Dependabot alerts are npm packages (minimatch) — not Rust. Likely `cargo audit` will pass cleanly.
- Should `ensure_payload_indexes` also create an index on `source_type` or `domain` with a more specific schema (e.g., for filtered searches)? Not needed now — only `url` and `domain` are used by `/facet`.

---

## Next Steps

- Monitor the new CI run to confirm both `security` and `mcp-smoke` jobs pass on the push to `feat/sidebar`
- If `cargo audit` surfaces actual Rust crate vulnerabilities, address them separately
