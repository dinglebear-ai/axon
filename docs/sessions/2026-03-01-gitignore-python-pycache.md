# Session: Gitignore Python __pycache__
**Date:** 2026-03-01
**Branch:** feat/sidebar
**Commit:** `6da73395`

---

## Session Overview

Short housekeeping session. `scripts/__pycache__/` was appearing in `git status` as modified/untracked. Added Python bytecode patterns to `.gitignore` and removed already-tracked `.pyc` files from the git index.

---

## Timeline

1. User reported `scripts/__pycache__/` showing in git status
2. Verified `.gitignore` had zero Python entries
3. Added `__pycache__/`, `*.pyc`, `*.pyo` section to `.gitignore`
4. User saw the file still appearing — diagnosed as already-tracked by git
5. Ran `git rm --cached -r scripts/__pycache__/` to untrack 6 `.pyc` files
6. Staged + committed + pushed

---

## Key Findings

- `.gitignore` had no Python entries at all prior to this session
- 6 `.pyc` files were already tracked: `audit_compose_images`, `check_qdrant_quality`, `enforce_monoliths`, `hook_justfile_lefthook_sync`, `qdrant-quality`, `test_qdrant_quality`
- `.gitignore` only prevents *new* files from being tracked; already-tracked files require `git rm --cached` to remove from the index

---

## Technical Decisions

- Added a broad `__pycache__/` pattern (not `scripts/__pycache__/`) — Python bytecode can appear in any directory; scoping to `scripts/` only would leave future directories unprotected
- Added `*.pyc` and `*.pyo` as belt-and-suspenders — covers bytecode compiled outside a `__pycache__/` dir (older Python versions)

---

## Files Modified

| File | Change |
|------|--------|
| `.gitignore` | Added Python artifacts section: `__pycache__/`, `*.pyc`, `*.pyo` |
| `scripts/__pycache__/*.cpython-314.pyc` (×6) | Removed from git index via `git rm --cached` |

---

## Commands Executed

```bash
# Check for existing Python entries in .gitignore
grep __pycache__ .gitignore  # → no matches

# Untrack all compiled bytecode
git rm --cached -r scripts/__pycache__/
# → rm 'scripts/__pycache__/audit_compose_images.cpython-314.pyc'
# → rm 'scripts/__pycache__/check_qdrant_quality.cpython-314.pyc'
# → rm 'scripts/__pycache__/enforce_monoliths.cpython-314.pyc'
# → rm 'scripts/__pycache__/hook_justfile_lefthook_sync.cpython-314.pyc'
# → rm 'scripts/__pycache__/qdrant-quality.cpython-314.pyc'
# → rm 'scripts/__pycache__/test_qdrant_quality.cpython-314.pyc'

# Commit and push
git add .gitignore
git commit -m "chore: gitignore Python __pycache__ and untrack compiled bytecode"
git push
# → feat/sidebar -> feat/sidebar (05175a96..6da73395)
```

---

## Behavior Changes (Before/After)

| Before | After |
|--------|-------|
| `scripts/__pycache__/*.pyc` tracked and showing in `git status` | Untracked; invisible to git |
| No Python patterns in `.gitignore` | `__pycache__/`, `*.pyc`, `*.pyo` ignored globally |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `git status` after commit | No `__pycache__` entries | Clean working tree | ✅ |
| `git push` | `feat/sidebar -> feat/sidebar` | `05175a96..6da73395` | ✅ |
| lefthook pre-commit | All hooks pass | env-guard ✔, monolith ✔, claude-symlinks ✔ | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations performed during this session.

---

## Risks and Rollback

- **Risk:** None material — removing bytecode from git index has no effect on runtime behavior
- **Rollback:** `git revert 6da73395` restores `.gitignore` to previous state and re-adds the `.pyc` files to the index

---

## Decisions Not Taken

- **`scripts/__pycache__/` scoped ignore** — rejected in favor of global `__pycache__/` pattern; scope-limiting would leave other directories unprotected

---

## Open Questions

- None

---

## Next Steps

- None — purely housekeeping; no follow-up required
