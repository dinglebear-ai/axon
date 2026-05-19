# Session Log — Legacy Removal and Guardrails (2026-02-19)

## 1. Session overview
- Removed legacy vector/crawl implementation files and switched dispatch paths to v2-only.
- Cleaned leftover v2 dead-code warnings by deleting unused scaffolding and test-gating test-only helpers.
- Added/verified CI and pre-commit guardrails: warnings-as-errors, legacy symbol deny-check, monolith policy updates.
- Applied GitHub branch protection on `main` requiring `fmt`, `clippy`, `test`, and `monolith` checks.

## 2. Timeline of major activities
- Completed v2-only cutover and deleted legacy files (`crates/vector/ops_legacy.rs`, `crates/jobs/crawl_jobs_legacy.rs`).
- Verified cutover with `cargo fmt --all`, `cargo check`, and full `cargo test` (all passing in observed output).
- Implemented CI split jobs and strict checks in `.github/workflows/ci.yml`.
- Implemented deny-list script (`scripts/enforce_no_legacy_symbols.py`) and added hook integration (`lefthook.yml`).
- Updated policy docs (`README.md`, `docs/monolith-policy.md`) to 500 LOC rust-only threshold and test/config exemptions.
- Applied and verified branch protection via GitHub API.

## 3. Key findings with `path:line` references
- CI now has separate required jobs and strict check flags: `.github/workflows/ci.yml:13`, `.github/workflows/ci.yml:41`, `.github/workflows/ci.yml:49`, `.github/workflows/ci.yml:61`, `.github/workflows/ci.yml:73`, `.github/workflows/ci.yml:85`.
- Warnings-as-errors for `cargo check` is set through `RUSTFLAGS`: `.github/workflows/ci.yml:68`.
- Monolith policy now enforces rust files only and 500-line threshold with config/test exemptions: `scripts/enforce_monoliths.py:21`, `scripts/enforce_monoliths.py:24`, `scripts/enforce_monoliths.py:25`.
- Legacy symbol deny-list is enforced by dedicated script with allowlist for guard tests: `scripts/enforce_no_legacy_symbols.py:10`, `scripts/enforce_no_legacy_symbols.py:18`, `scripts/enforce_no_legacy_symbols.py:31`.
- Pre-commit now runs deny-check before monolith/rustfmt/clippy: `lefthook.yml:4`.
- Documentation aligned with implemented policy: `README.md:404`, `README.md:406`, `docs/monolith-policy.md:13`, `docs/monolith-policy.md:18`, `docs/monolith-policy.md:34`.

## 4. Technical decisions and rationale
- Removed dual-stack fallback code to prevent backsliding; v2 is the only runtime path.
- Kept “no legacy reference” tests to guard against regressions while allowing their string literals.
- Scoped deny-check scan roots to code/config/tooling paths to avoid false positives from historical docs/session notes.
- Split CI into explicit jobs so branch protection can require named checks directly.
- Enforced warning-failure in CI to prevent slow quality drift.

## 5. Files modified/created and purpose
- `.github/workflows/ci.yml`: split CI jobs (`monolith`, `no-legacy-symbols`, `fmt`, `check`, `clippy`, `test`, `security`) and strict warning behavior.
- `scripts/enforce_monoliths.py`: monolith policy now rust-file focused, 500 LOC threshold, test/config exemptions.
- `scripts/enforce_no_legacy_symbols.py` (created): deny-list checker for legacy symbols.
- `lefthook.yml`: added `no-legacy-symbols` pre-commit command.
- `README.md`: updated guardrail documentation.
- `docs/monolith-policy.md`: updated policy details and deny-check command.
- `crates/vector/ops_legacy.rs` (deleted): removed legacy implementation.
- `crates/jobs/crawl_jobs_legacy.rs` (deleted): removed legacy implementation.
- `crates/jobs/crawl_jobs/config.rs` (deleted): removed unused v2 scaffolding.

## 6. Critical commands executed and outcomes
- `cargo fmt --all` -> passed.
- `cargo check` -> passed (after warning cleanup).
- `cargo test` -> passed (`86` unit tests plus integration/doc tests shown as pass).
- `python3 scripts/enforce_no_legacy_symbols.py` -> initially failed on docs/history files, then passed after scanner scope update.
- `python3 scripts/enforce_monoliths.py --staged` -> passed.
- `gh api ... /branches/main/protection` (PUT with JSON payload) -> succeeded; required contexts set.
- `gh api repos/jmagar/axon_rust/branches/main/protection --jq ...` -> confirmed strict=true and contexts `[fmt, clippy, test, monolith]`.
- `git push` on `chore/housekeeping` -> succeeded (`4098d22..8358a6b`, then later commit `2d1bf0f` observed in log).

## 7. Behavior changes (before/after)
- Before: vector/crawl dispatch supported legacy/v2 selection paths.
  After: legacy paths removed; v2-only dispatch.
- Before: CI had combined `check` flow.
  After: CI exposes discrete required checks (`fmt`, `clippy`, `test`, `monolith`) plus `no-legacy-symbols`.
- Before: monolith file checks covered multiple file types and 400-line threshold.
  After: monolith file checks apply to changed `*.rs` only, threshold 500, with test/config exemptions.
- Before: legacy symbols could be reintroduced unless caught manually.
  After: deny-list automation blocks reintroduction in hooks/CI.

## 8. Verification evidence (`command | expected | actual | status`)
- `cargo check | build succeeds with no warnings targeted by new policy | Finished dev profile successfully | PASS`
- `cargo test | test suite passes | 86 passed; 0 failed (plus integration/doc tests passed) | PASS`
- `python3 scripts/enforce_no_legacy_symbols.py | no banned symbols in scanned code paths | "Legacy symbol deny-check passed." | PASS`
- `python3 scripts/enforce_monoliths.py --staged | no monolith violations in staged diff | "Monolith policy check passed." | PASS`
- `gh api .../branches/main/protection --jq ... | required checks enforced on main | {"strict":true,"contexts":["fmt","clippy","test","monolith"],"enforce_admins":true,"approvals":1} | PASS`

## 9. Source IDs + collections touched (embed/retrieve source IDs, collections, outcomes)
- `axon embed "docs/sessions/2026-02-19-legacy-removal-and-guardrails.md" --json` -> `{"job_id":"9c8a5d1f-6d00-402f-bce7-8d176b59b9e5","status":"pending","source":"rust"}` and Lapin/Tokio shutdown errors printed.
- `data.url` and `data.collection` were not present in embed output; `axon embed --wait true --json` and `axon status --json` attempts produced no stdout in this environment during polling windows.
- Retrieve attempt using available source and empty collection value: `axon retrieve "rust" --collection ""` -> 404 on `/collections//points/scroll`.
- Additional retrieve attempt without explicit collection: `axon retrieve "rust"` -> `No content found for URL: rust`.
- Outcome: Axon partial failure (`embed accepted/pending`, verify unsuccessful due missing embed fields and retrieve miss).

## 10. Risks and rollback
- Risk: deny-check false positives if scanning too broadly.
  Mitigation: scanner scope restricted to `crates/tests/scripts/.github` and known allowlist files.
- Risk: stricter CI (`-D warnings`) can block merges on new warnings.
  Mitigation: intentional quality gate; fix-forward expected.
- Rollback path: revert commits containing guardrail changes (`8358a6b`, `2d1bf0f`) or disable specific CI jobs temporarily.

## 11. Decisions not taken
- Did not re-enable environment variable toggles for legacy fallback.
- Did not include docs/session directories in deny-check scan scope after false-positive finding.
- Did not lower monolith threshold below 500 for rust files in this change.

## 12. Open questions
- Should `no-legacy-symbols` also be required in branch protection contexts (currently enforced in CI but not listed as required context)?
- Should branch protection also enforce conversation resolution / linear history (currently not enabled in observed response)?
- Should `cargo test --all` remain single-job or be split (unit/integration) for faster feedback?

## 13. Next steps
- Optionally add `no-legacy-symbols` to required status checks on `main`.
- Optionally enforce additional branch protection toggles (conversation resolution, linear history) via API.
- Keep guard tests updated if deny-list symbols or allowlist paths evolve.
