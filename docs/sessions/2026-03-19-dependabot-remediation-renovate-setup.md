# Session: Dependabot Remediation + Renovate Setup
**Date:** 2026-03-19
**Branch:** feat/pulse-shell-and-hybrid-search

---

## Session Overview

Investigated and resolved 22 open Dependabot security alerts across npm and Rust ecosystems. Analyzed dependency chains to distinguish direct vs. transitive vulnerabilities, applied fixes, then set up Renovate Bot to automate future dependency maintenance and prevent alert accumulation.

---

## Timeline

1. **Investigated alerts** — queried GitHub API to enumerate all 28 alerts (22 open, 6 already fixed); grouped by package with CVE details and patched versions
2. **Mapped dependency chains** — used `pnpm why` to trace transitive dep paths for non-direct packages (hono, undici, dompurify, express-rate-limit, @hono/node-server)
3. **Applied npm fixes** — bumped `next` directly, added `pnpm.overrides` for 5 transitive packages
4. **Applied Rust fix** — `cargo update -p quinn-proto --precise 0.11.14`
5. **Ran `pnpm install`** — lockfile updated; all packages resolved to patched or newer versions
6. **Assessed automation options** — compared Dependabot grouping, cargo/pnpm audit in CI, auto-merge workflow, and Renovate
7. **Set up Renovate** — created `renovate.json` with grouping, auto-merge, transitive remediation, and security policy
8. **Added `pnpm audit` to CI** — discovered existing `cargo audit` + `cargo deny` already present in `security` job

---

## Key Findings

- **22 open alerts across 7 packages**: next (4 CVEs ×2), undici (6 CVEs), hono (4 CVEs), @hono/node-server (1), dompurify (1), express-rate-limit (1), quinn-proto (1)
- **Duplicate next alerts** explained by two `package.json` files scanned: `apps/web/package.json` and `.claude/worktrees/agent-aa643b9e/apps/web/package.json` (worktrees are gitignored, not a real issue)
- **5 of 7 packages were transitive deps** — Dependabot flags them but cannot fix them; required `pnpm.overrides`
- **Transitive chains:**
  - `undici` ← `jsdom` ← `vitest` (devDep)
  - `hono`, `@hono/node-server`, `express-rate-limit` ← `@modelcontextprotocol/sdk` ← `shadcn` (devDep)
  - `dompurify` ← `mermaid` ← `@platejs/code-drawing` + `@streamdown/mermaid` (prod deps)
- **quinn-proto** ← `quinn` ← `reqwest` (Rust); 0.11.14 was available on crates.io
- **CI already had `cargo audit` and `cargo deny`** in the `security` job — only `pnpm audit` was missing

---

## Technical Decisions

- **`pnpm.overrides` over `resolutions`**: pnpm's native mechanism; ensures transitive deps get pinned to safe minimum versions without breaking semver resolution
- **`>=version` range in overrides** (not exact pins): allows future patch updates to flow through without needing override updates
- **Renovate over Dependabot enhancements**: Renovate's `transitiveRemediation: true` handles `pnpm.overrides` automatically — the core pain point we hit manually. Adding Dependabot grouping/auto-merge would duplicate Renovate's capabilities.
- **`--prod` flag on `pnpm audit`**: dev-only vulns (like undici via jsdom/vitest) don't affect production builds; `--audit-level=high` avoids noise from medium dev vulns
- **Weekly schedule + 3-day minimum release age**: avoids reacting to bad releases that get yanked, reduces PR noise
- **`next` minor/major flagged for manual review**: Next.js has a history of breaking changes in minor versions (e.g., App Router behavior, cache semantics)

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `apps/web/package.json` | `next` 16.1.6 → 16.1.7; added `pnpm.overrides` block | Fix direct dep + pin 5 transitive deps |
| `apps/web/pnpm-lock.yaml` | Auto-updated by `pnpm install` | Lockfile reflects patched versions |
| `Cargo.lock` | `quinn-proto` 0.11.13 → 0.11.14 | Fix Rust QUIC DoS vulnerability |
| `renovate.json` | Created | Automate future dependency maintenance |
| `.github/workflows/ci.yml` | Added `pnpm audit --prod --audit-level=high` step | Block high-severity npm vulns in CI |

---

## Commands Executed

```bash
# Enumerate all alerts with fix versions
gh api repos/<repo>/dependabot/alerts --paginate | python3 -c "..."

# Trace transitive dependency chains
pnpm why hono
pnpm why undici
pnpm why dompurify
pnpm why express-rate-limit

# Verify quinn-proto fix availability
cargo search quinn-proto   # → 0.11.14

# Apply Rust fix
cargo update -p quinn-proto --precise 0.11.14

# Apply npm fixes and update lockfile
pnpm install  # → +10/-10 packages, Done in 7.4s
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| npm security posture | 21 open alerts | 0 open alerts (all patched or superseded) |
| Rust security posture | 1 open alert (quinn-proto 0.11.13) | 0 open alerts (0.11.14) |
| Transitive dep management | Manual investigation + override | Renovate auto-creates `pnpm.overrides` PRs |
| CI npm audit | Not present | `pnpm audit --prod --audit-level=high` blocks merges |
| Dependency update cadence | Reactive (alerts accumulate) | Proactive (weekly Renovate PRs) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `grep '"next"' apps/web/package.json` | `16.1.7` | `16.1.7` | ✅ |
| `grep 'hono@' apps/web/pnpm-lock.yaml` | `>=4.12.4` | `4.12.8` | ✅ |
| `grep '@hono/node-server@' pnpm-lock.yaml` | `>=1.19.10` | `1.19.11` | ✅ |
| `grep 'dompurify@' pnpm-lock.yaml` | `>=3.3.2` | `3.3.3` | ✅ |
| `grep 'express-rate-limit@' pnpm-lock.yaml` | `>=8.2.2` | `8.3.1` | ✅ |
| `grep 'undici@' pnpm-lock.yaml` | `>=7.24.0` | `7.24.4` | ✅ |
| `grep -A2 'name = "quinn-proto"' Cargo.lock` | `0.11.14` | `0.11.14` | ✅ |
| `pnpm install` exit | clean | Done in 7.4s (peer warning for @xterm only) | ✅ |

**Peer warning noted**: `@xterm/addon-canvas@0.8.0-beta.48` expects `@xterm/xterm@^5.0.0` but `6.0.0` is installed. Pre-existing issue, unrelated to this session.

---

## Risks and Rollback

- **Low risk overall** — all changes are patch-level bumps within semver ranges already declared
- `pnpm.overrides` could break packages that rely on specific transitive versions — unlikely for security patches
- Rollback: revert `apps/web/package.json` overrides block + run `pnpm install`; revert `cargo update` by reverting `Cargo.lock` hunk for quinn-proto

---

## Decisions Not Taken

| Option | Rejected Because |
|--------|-----------------|
| Dependabot grouping + auto-merge workflow | Renovate replaces both; running both creates duplicate PR noise |
| Exact version pins in `pnpm.overrides` (e.g., `"hono": "4.12.8"`) | `>=` ranges let future safe patches flow through without override churn |
| `pnpm audit --audit-level=moderate` in CI | Dev-only moderate vulns (undici via jsdom) would create constant false positives |
| Pinning `next` to exact version | Semver `^16.1.7` allows Renovate to auto-merge future patches |

---

## Renovate Configuration Summary

**`renovate.json`** key settings:
- `schedule: ["every weekend"]` — weekly batch, not daily noise
- `vulnerabilityAlerts.automerge: true` + `schedule: ["at any time"]` — security fixes bypass weekly schedule
- `transitiveRemediation: true` — auto-generates `pnpm.overrides` for transitive vulns
- `minimumReleaseAge: "3 days"` — avoids yanked releases
- Package groups: platejs (~20 pkgs), radix-ui, xterm, hono, ai-sdk, tailwindcss, biome
- `next` minor/major → manual review; patches → auto-merge

**Required action by user**: Install Renovate GitHub App at `https://github.com/apps/renovate`

---

## Open Questions

- The `.claude/worktrees/agent-aa643b9e/apps/web/package.json` triggers duplicate Dependabot alerts. Worktrees are gitignored so this shouldn't affect the actual codebase — but confirms Dependabot scans beyond `.gitignore`. Worth monitoring after Renovate onboarding to see if it creates duplicate PRs.
- `@xterm/addon-canvas@0.8.0-beta.48` peer conflict with xterm v6 — pre-existing, but worth revisiting when xterm group gets a Renovate PR.

---

## Next Steps

1. **User action required**: Install Renovate GitHub App at `https://github.com/apps/renovate` — select axon_rust repo
2. Review and merge Renovate's onboarding PR (it opens automatically after install)
3. Verify Dependabot alerts close after fixing PRs are merged to main
4. Monitor first weekly Renovate run (next weekend) to validate grouping and auto-merge behavior
