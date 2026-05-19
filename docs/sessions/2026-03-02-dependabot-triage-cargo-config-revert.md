# Session: Dependabot Triage + .cargo/config.toml Revert

**Date:** 2026-03-02
**Branch:** feat/sidebar
**Commits:** `149325f0`, `fa8ddc29`

---

## 1. Session Overview

Investigated two open items flagged in the previous session doc:

1. **2 Dependabot high-severity alerts on `main`** — both `minimatch` ReDoS CVEs, fixed by bumping the lockfile from 10.2.2 → 10.2.4.
2. **`.cargo/config.toml` deletion** — confirmed intentional. `~/.cargo/config.toml` already provides `sccache`, `mold` linker, and `split-debuginfo` globally. The project-level file was a redundant duplicate. An erroneous restoration (`149325f0`) was immediately reverted (`fa8ddc29`).

Net result: one real fix (minimatch), one false alarm (cargo config), one reverted mistake.

---

## 2. Timeline

| Step | Activity | Outcome |
|------|----------|---------|
| 1 | Fetch Dependabot alerts via `gh api` | 2 open high-severity alerts — both `minimatch`, both `pnpm-lock.yaml` |
| 2 | Recover deleted `.cargo/config.toml` from git history | Contents: `[build]\nrustc-wrapper = "sccache"` only |
| 3 | Check `~/.cargo/config.toml` | Has sccache + mold linker + split-debuginfo — deletion was correct |
| 4 | Trace minimatch dep chain | `shadcn@3.8.5 → ts-morph@26 → @ts-morph/common@0.27 → minimatch@10.2.2` |
| 5 | Check if ts-morph@27 fixes it | Still uses `minimatch: "^10.0.1"` — lockfile update sufficient |
| 6 | `pnpm update minimatch` | 10.2.2 → 10.2.4 |
| 7 | Commit `149325f0` | Both `.cargo/config.toml` restore (wrong) + minimatch fix |
| 8 | User corrects: global config makes project config redundant | |
| 9 | `git rm .cargo/config.toml` + commit `fa8ddc29` | Reverts the erroneous restoration |

---

## 3. Key Findings

- **Dependabot alerts #2 + #3**: Both are `minimatch` ReDoS vulnerabilities (`>= 10.0.0, < 10.2.3`). Fixed in `10.2.3+`. Latest is `10.2.4`.
- **Minimatch is dev-only**: Used exclusively by `shadcn` CLI (component installer). No runtime exposure — attack surface is limited to local developer machines running `pnpm dlx shadcn add`.
- **`@ts-morph/common@0.28.1`** (in ts-morph@27) still declares `minimatch: "^10.0.1"` — upgrading ts-morph is unnecessary; a lockfile update resolves it directly.
- **`~/.cargo/config.toml` covers everything**: `[build] rustc-wrapper = "sccache"` + `[target.x86_64-unknown-linux-gnu] linker = "clang"` + `rustflags = ["-C", "link-arg=-fuse-ld=mold"]` + `[profile.dev] split-debuginfo = "unpacked"`. Project-level file was a subset duplicate.
- **Dependabot alerts still show on push**: Expected — alerts are against `main`; fix is on `feat/sidebar`. Will auto-close on merge.

---

## 4. Technical Decisions

**Lockfile update only (not ts-morph upgrade):**
`@ts-morph/common` uses `minimatch: "^10.0.1"` (semver range allows 10.2.4). Running `pnpm update minimatch` bumps only the lockfile entry without touching any direct dependencies. Upgrading ts-morph to v27 would be a larger change with potential breaking API changes — unnecessary when the lockfile update achieves the same security outcome.

**Revert rather than amend:**
The erroneous `.cargo/config.toml` restoration was already pushed. A revert commit (`fa8ddc29`) preserves the accurate history — the file was added, then removed when the mistake was understood. Amending would have required a force push.

---

## 5. Files Modified

| File | Change |
|------|--------|
| `apps/web/pnpm-lock.yaml` | `minimatch` 10.2.2 → 10.2.4 (in `149325f0`, kept in `fa8ddc29`) |
| `.cargo/config.toml` | Added in `149325f0` (mistake), removed in `fa8ddc29` (revert) — net: absent |

---

## 6. Commands Executed

```bash
# Fetch Dependabot alerts
gh api repos/jmagar/axon_rust/dependabot/alerts --jq '...'
# → 2 alerts: minimatch >= 10.0.0, < 10.2.3

# Recover deleted file
git show ddc19a0a:".cargo/config.toml"
# → [build]\nrustc-wrapper = "sccache"

# Check global config
cat ~/.cargo/config.toml
# → sccache + mold + split-debuginfo

# Trace dep chain
pnpm why minimatch
# → shadcn@3.8.5 → ts-morph@26.0.0 → @ts-morph/common@0.27.0 → minimatch@10.2.2

# Check ts-morph@27 deps
pnpm info ts-morph@27.0.2 dependencies
# → @ts-morph/common@~0.28.1 — still uses minimatch ^10.0.1

# Fix minimatch
pnpm update minimatch
# → 10.2.2 → 10.2.4

# Verify
pnpm why minimatch
# → minimatch 10.2.4

# Revert cargo config
git rm .cargo/config.toml
git commit -m "revert: remove redundant .cargo/config.toml..."
```

---

## 7. Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| `minimatch` version in lockfile | 10.2.2 (vulnerable to ReDoS) | 10.2.4 (patched) |
| `.cargo/config.toml` | Absent (correctly deleted) → briefly restored → absent again | Absent — global `~/.cargo/config.toml` handles all build config |
| Dependabot alerts on `main` | 2 open (high) | Will close on merge of `feat/sidebar` |

---

## 8. Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm why minimatch` (after update) | `minimatch 10.2.4` | `minimatch 10.2.4` | ✅ |
| `cargo check` (pre-commit, no `.cargo/config.toml`) | Passes — global config provides sccache | Finished in 3.86s | ✅ |
| `git push fa8ddc29` | Branch updated | `149325f0..fa8ddc29 feat/sidebar -> feat/sidebar` | ✅ |

---

## 9. Source IDs + Collections Touched

*(This doc — see post-save embed step)*

---

## 10. Risks and Rollback

- **minimatch update**: Lockfile-only change. Rollback: `git checkout apps/web/pnpm-lock.yaml`. Risk: none — patch-level semver bump within declared range.
- **No `.cargo/config.toml` in repo**: Any developer without `~/.cargo/config.toml` set up will not use sccache or mold. This is the same situation as before the file was ever added — acceptable for a personal homelab project.

---

## 11. Decisions Not Taken

- **Upgrade ts-morph to v27**: Would also resolve the minimatch version transitively, but introduces a larger dep change with potential API breaks. Lockfile bump is simpler and equivalent.
- **Add `overrides`/`pnpm.overrides` to force minimatch version**: Not needed when `pnpm update` resolved it cleanly within the existing semver range.
- **Re-add `.cargo/config.toml` for portability**: Rejected — the project targets a single machine with a known global config. Adding a project-level file would be documentation of the global state, not a functional improvement.

---

## 12. Open Questions

- Dependabot alerts reference `main` — will they auto-close after `feat/sidebar` is merged, or does GitHub require a manual dismiss?

---

## 13. Next Steps

- Merge `feat/sidebar` → `main` to close the Dependabot alerts.
- Any new developer onboarding should set up `~/.cargo/config.toml` with sccache + mold (not documented in README yet).
