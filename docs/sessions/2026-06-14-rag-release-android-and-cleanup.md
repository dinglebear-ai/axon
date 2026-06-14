---
date: 2026-06-14 06:59:28 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: e897be76
session id: c967cb21-fffb-47a4-b826-69c8d94666ec
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon/c967cb21-fffb-47a4-b826-69c8d94666ec.jsonl
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon e897be76 [main]
beads: axon_rust-l4dd
---

# RAG, releases, Android crash, and repo cleanup session

## User Request

The session began with a request to systematically debug poor synthesized RAG answers, then expanded into reviewing and fixing retrieval/synthesis/ranking, docs and action references, release automation, Android APK release/crash handling, mobile testing skill documentation, repo cleanup, and finally saving this session log.

## Session Overview

- Improved Axon ask/research/summarize quality through retrieval context and synthesis changes, including broader full-document context for GPT/Gemini/Claude-class models.
- Configured and verified Gemini, Codex app-server, and OpenAI-compatible LLM backend behavior across ask/research/summarize paths, with PR #213 observed as the Codex app-server backend work branch.
- Updated docs around actions/commands, release behavior, indexed-document stats, and mobile testing expectations.
- Cut Android releases and debugged the launch crash down to release/R8 Navigation Compose typed route serializer stripping; fixed and released Android v1.3.
- Used Labby `claude-in-mobile` plus a headless Android emulator to prove the published APK launches cleanly, then documented that recovery path in the Lab testing skill.
- Ran repo-status cleanup; only safe prune operations were no-ops. Dirty or active worktrees were intentionally preserved.

## Sequence of Events

1. Investigated bad synthesized answers by reviewing retrieval context, selected sources, and synthesis behavior.
2. Tightened retrieval/context behavior so higher-capacity models receive more full documents and source selection avoids collapsing into near-duplicate citations.
3. Verified and configured multiple LLM synthesis paths, including Gemini, Codex app-server, and OpenAI-compatible backends.
4. Reviewed retrieval, synthesis, and ranking code with the requested review workflows, addressed surfaced issues, and merged reviewed work.
5. Swept docs for stale command/action references and generated richer action documentation.
6. Checked release automation, triggered releases from tags/main pushes, and verified release workflow behavior.
7. Reset Axon runtime state as requested, then cut Android releases.
8. Reproduced the Android launch crash on a headless emulator through Labby `claude-in-mobile`, retraced the release stack, and identified R8/serializer stripping as the cause.
9. Patched Android release rules and route serial names, built and tested minified release APKs, and published Android v1.3.
10. Updated the Lab `claude-in-mobile` testing skill so agents do not stop at "no device"; they now know to expose host ADB and boot/create a headless emulator.
11. Ran repo cleanup evidence collection and safe prune commands, then created a follow-up bead for the remaining conflicting PR #212.
12. Wrote this session log as a path-limited documentation artifact.

## Key Findings

- The poor RAG answer was not caused by missing source content; selected sources contained enough procedural detail, but the synthesis/context path fed too little full-document context and allowed near-duplicate evidence to dominate citations.
- The Android v1.2 release still crashed on launch. Retrace showed `kotlinx.serialization.SerializationException: Serializer for class 'z' is not found` from `AxonNavGraph.kt` through Navigation Compose typed route setup.
- The Android crash was release-only behavior from R8/minification around typed Navigation Compose route serializers, not the earlier crypto dependency issue.
- Labby `claude-in-mobile` was connected, but Android was initially unavailable because host ADB was bound only to loopback and no emulator/device was attached.
- `adb -a start-server` plus a headless AVD made Labby see `emulator-5554`; after that, Labby MCP app tools installed and launched APKs successfully.
- `codex/fix-source-doc-char-boundary` is merged into `origin/main`, but its worktree has uncommitted edits in source and test files, so deleting it is not safe.
- PR #212 is open and GitHub reports it as `CONFLICTING`; a follow-up bead was created to track that explicit remaining work.

## Technical Decisions

- For high-capacity synthesis models, spend tokens up front on multiple full docs rather than optimizing for a smaller context that produces incomplete answers.
- Preserve and cite multiple selected full docs, or deduplicate near-identical docs before synthesis, so final answers do not effectively cite only one source.
- Use the existing Codex app-server child-process adapter for the first Codex LLM backend slice instead of introducing a desktop socket transport.
- Keep Codex app-server runtime isolated from the user's live Codex hooks, skills, MCP config, and app config.
- Fix Android launch by keeping the small typed navigation route package and giving typed routes stable `@SerialName` values instead of disabling minification broadly.
- Document the Labby mobile recovery path in the Lab testing skill rather than relying on agent memory.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `apps/android/app/build.gradle.kts` | - | Bumped Android release to `versionName=1.3`, `versionCode=4`. | Commit `5c028b88`; release `android-v1.3`. |
| modified | `apps/android/app/proguard-rules.pro` | - | Kept typed nav route serializers/classes for release/R8 builds. | Commit `5c028b88`; crash buffer empty after release install. |
| modified | `apps/android/app/src/main/java/com/axon/app/ui/nav/AxonNav.kt` | - | Added stable route serial names. | Commit `5c028b88`. |
| modified | `apps/android/app/src/main/java/com/axon/app/ui/nav/AxonNavGraph.kt` | - | Added stable route serial names for typed destinations. | Commit `5c028b88`. |
| modified | `plugins/testing/skills/claude-in-mobile/SKILL.md` | - | Documented Labby Android recovery path in Lab repo. | Lab commit `4defbcee`. |
| modified | `plugins/testing/skills/claude-in-mobile/references/tooling.md` | - | Added ADB bridge and emulator boot instructions in Lab repo. | Lab commit `4defbcee`. |
| modified | `.env.example` | - | Documented Codex app-server LLM backend envs in PR #213 branch. | Observed in `codex/codex-app-server-llm-backend` diff. |
| modified | `CLAUDE.md` | - | Documented LLM backend behavior in PR #213 branch. | Observed in `codex/codex-app-server-llm-backend` diff. |
| modified | `config.example.toml` | - | Added Codex backend config examples in PR #213 branch. | Observed in branch diff and dirty source-doc worktree. |
| modified | `docs/guides/configuration.md` | - | Updated configuration docs. | Observed in PR #213 branch and dirty source-doc worktree. |
| modified | `docs/reference/env-matrix.md` | - | Updated env reference. | Observed in PR #213 branch. |
| modified | `docs/reference/env-matrix.toml` | - | Updated env matrix. | Observed in PR #213 branch and dirty worktrees. |
| created | `docs/sessions/2026-06-14-codex-app-server-llm-backend.md` | - | Saved prior Codex app-server backend session log on PR #213 branch. | Branch head `77f03679`. |
| created | `docs/superpowers/plans/2026-06-14-codex-app-server-llm-backend.md` | - | Implementation plan for Codex app-server backend. | Present as untracked in main; present in PR #213 branch diff. |
| modified | `scripts/check-env-config-boundary.py` | - | Extended env/config boundary validation. | Observed in PR #213 branch diff. |
| modified | `src/core/config/parse/build_config/config_literal.rs` | - | Parsed Codex backend envs. | Observed in PR #213 branch diff. |
| modified | `src/core/config/parse/build_config/tests/env_required.rs` | - | Added env-required tests. | Observed in PR #213 branch diff. |
| modified | `src/core/config/parse/env_registry/runtime.rs` | - | Classified Codex runtime env vars. | Observed in PR #213 branch diff. |
| modified | `src/core/config/parse/tuning.rs` | - | Derived model tier for configured backend models. | Observed in PR #213 branch diff. |
| modified | `src/core/config/parse/tuning_tests.rs` | - | Added tuning/model-tier tests. | Observed in PR #213 branch diff. |
| modified | `src/core/config/types/config.rs` | - | Added Codex config fields. | Observed in PR #213 branch diff. |
| modified | `src/core/config/types/config_impls.rs` | - | Added defaults/debug fields. | Observed in PR #213 branch diff. |
| modified | `src/core/llm.rs` | - | Dispatched Codex app-server completions. | Observed in PR #213 branch diff. |
| modified | `src/core/llm/codex_app_server.rs` | - | Hardened Codex app-server adapter. | Observed dirty in PR #213 worktree. |
| modified | `src/core/llm/codex_app_server/home.rs` | - | Hardened isolated Codex home handling. | Observed dirty in PR #213 worktree. |
| modified | `src/core/llm/codex_app_server/home_tests.rs` | - | Added/updated isolated-home tests. | Observed dirty in PR #213 worktree. |
| modified | `src/core/llm/codex_app_server/protocol.rs` | - | Updated app-server protocol parsing. | Observed in PR #213 branch diff. |
| modified | `src/core/llm/codex_app_server/protocol_tests.rs` | - | Added/updated protocol tests. | Observed dirty in PR #213 worktree. |
| modified | `src/core/llm/codex_app_server_tests.rs` | - | Added/updated Codex backend tests. | Observed dirty in PR #213 worktree. |
| modified | `src/core/llm/concurrency.rs` | - | Added Codex limiter key. | Observed in PR #213 branch diff. |
| modified | `src/core/llm/headless/common.rs` | - | Reused shared timeout helpers. | Observed dirty in PR #213 worktree. |
| modified | `src/core/llm/headless/common_tests.rs` | - | Added shared helper tests. | Observed dirty in PR #213 worktree. |
| modified | `src/core/llm/headless/gemini.rs` | - | Reused common headless helpers. | Observed in PR #213 branch diff. |
| modified | `src/core/llm/headless/gemini_tests.rs` | - | Adjusted Gemini tests. | Observed in PR #213 branch diff. |
| modified | `src/core/llm/openai_compat_tests.rs` | - | Adjusted OpenAI-compatible tests. | Observed in PR #213 branch diff. |
| modified | `src/core/llm/types.rs` | - | Added Codex backend kind/config/model selection. | Observed in PR #213 branch diff. |
| modified | `src/core/llm/types_tests.rs` | - | Added backend/config tests. | Observed in PR #213 branch diff. |
| modified | `src/core/llm_backend_tests.rs` | - | Added backend dispatch tests. | Observed in PR #213 branch diff. |
| modified | `src/jobs/config_snapshot.rs` | - | Preserved Codex backend fields across jobs. | Observed dirty in PR #213 worktree. |
| modified | `src/jobs/workers/runners_tests.rs` | - | Added worker/job snapshot tests. | Observed dirty in PR #213 worktree. |
| modified | `src/services/search/synthesis/source.rs` | - | Preserved full research sources for GPT/Codex-class models. | Observed in PR #213 branch diff. |
| modified | `src/services/search/synthesis_tests.rs` | - | Added synthesis source tests. | Observed in PR #213 branch diff. |
| modified | `src/vector/ops/commands/ask.rs` | - | Validated Codex config for ask. | Observed in PR #213 branch diff. |
| modified | `src/vector/ops/commands/ask/context.rs` | - | Included Codex in high-context detection. | Observed in PR #213 branch diff. |
| modified | `src/vector/ops/commands/ask/context_tests.rs` | - | Added context tests. | Observed in PR #213 branch diff. |
| modified | `src/vector/ops/commands/ask/synthesis_prompt.rs` | - | Used direct synthesis prompt for Codex. | Observed in PR #213 branch diff. |
| modified | `src/vector/ops/commands/ask/synthesis_prompt_tests.rs` | - | Added synthesis prompt tests. | Observed in PR #213 branch diff. |
| modified | `src/vector/ops/commands/ask_tests.rs` | - | Added ask validation tests. | Observed in PR #213 branch diff. |
| modified | `src/vector/ops/commands/streaming_tests.rs` | - | Added streaming/backend tests. | Observed in PR #213 branch diff. |
| modified | `tests/compose_env_contract.rs` | - | Added env contract coverage. | Observed in PR #213 branch diff. |
| modified | `README.md` | - | Documented release updater in PR #211 branch. | Observed before PR #211 merge. |
| modified | `docs/operations/deployment.md` | - | Documented release updater deployment flow. | Observed before PR #211 merge. |
| created | `docs/sessions/2026-06-13-axon-update-release-sync.md` | - | Saved release-updater session log. | Observed before PR #211 merge. |
| created | `docs/superpowers/plans/2026-06-13-axon-update-release-sync.md` | - | Release-updater implementation plan. | Observed before PR #211 merge. |
| modified | `src/cli/commands.rs` | - | Added update command dispatch. | Observed before PR #211 merge. |
| created | `src/cli/commands/update.rs` | - | Implemented release binary updater. | Observed before PR #211 merge. |
| created | `src/cli/commands/update_tests.rs` | - | Added updater tests. | Observed before PR #211 merge. |
| modified | `src/core/config/cli.rs` | - | Added update CLI config. | Observed before PR #211 merge. |
| modified | `src/core/config/help.rs` | - | Added help text. | Observed before PR #211 merge. |
| modified | `src/core/config/parse/build_config.rs` | - | Added build-config parse plumbing. | Observed before PR #211 merge. |
| modified | `src/core/config/parse/build_config/command_dispatch.rs` | - | Added command dispatch. | Observed before PR #211 merge. |
| modified | `src/core/config/parse/env_registry/advanced.rs` | - | Added updater env docs. | Observed before PR #211 merge. |
| modified | `src/core/config/parse_tests.rs` | - | Added parse tests. | Observed before PR #211 merge. |
| modified | `src/core/config/types/enums.rs` | - | Added command enum. | Observed before PR #211 merge. |
| modified | `src/core/config/types_tests.rs` | - | Added config type tests. | Observed before PR #211 merge. |
| modified | `src/lib.rs` | - | Wired update command. | Observed before PR #211 merge. |
| created | `docs/sessions/2026-06-14-rag-release-android-and-cleanup.md` | - | This session artifact. | Created by `save-to-md` workflow. |

## Beads Activity

| bead | title | action(s) | final status | why it mattered |
|---|---|---|---|---|
| `axon_rust-l4dd` | Resolve PR #212 spider examples conflicts | Created during save pass. | open | Tracks the concrete remaining open PR that GitHub reports as conflicting. |
| `axon_rust-it6j` | Codex app-server backend umbrella | Observed as closed in beads interactions. | closed | Recent interactions say all Codex app-server backend tasks were completed in PR #213. |
| `axon_rust-it6j.1` through `axon_rust-it6j.8` | Codex app-server backend children | Observed as closed in beads interactions. | closed | Recent interactions say each child task was implemented in PR #213 and verified. |
| `axon_rust-ljg7` | Markdown planner char-boundary panic | Observed as closed in beads interactions. | closed | Confirms the source-doc char-boundary fix had been completed and verified before this save pass. |

## Repository Maintenance

### Plans

- `find docs/plans -maxdepth 2 -type f` showed many historical plans and completed plans already under `docs/plans/complete/`.
- No plan files were moved. The current untracked `docs/superpowers/plans/2026-06-14-codex-app-server-llm-backend.md` was not moved because it is untracked in `main` and connected to active PR #213 evidence.
- `.claude/current-plan` reported `/home/jmagar/workspace/axon_rust/docs/plans/2026-05-27-android-phase2-stubbed-modes.md`, which is outside this repository checkout and was not modified.

### Beads

- Ran `bd list --all --sort updated --reverse --limit 100 --json` and `tail -200 .beads/interactions.jsonl`.
- Created `axon_rust-l4dd` for unresolved PR #212 conflicts.
- Did not close additional beads during this save pass because no untracked remaining work was completed by the save operation itself.

### Worktrees and branches

- `git worktree list --porcelain` showed three worktrees: `main`, `codex/codex-app-server-llm-backend`, and `codex/fix-source-doc-char-boundary`.
- `codex/codex-app-server-llm-backend` has open PR #213 and dirty files, so it was not removed.
- `codex/fix-source-doc-char-boundary` is merged into `origin/main`, but its worktree has uncommitted edits in `config.example.toml`, `docs/guides/configuration.md`, `docs/reference/env-matrix.toml`, and source-doc/TEI planner files. It was not removed.
- `git remote prune origin --dry-run` and `git worktree prune --dry-run --verbose` showed no safe cleanup targets. Earlier safe prune commands were no-ops.

### Stale docs

- The Lab `claude-in-mobile` skill docs were updated earlier in the session to document ADB bridge and emulator recovery.
- No additional stale docs were updated during this save pass beyond this session artifact.

### Transparency

- Dirty worktrees were preserved.
- Open PR branches were preserved.
- The untracked Codex app-server plan in `main` was preserved.
- Current `main` CI was still in progress when checked via `gh run list`; Docker image and auto-tag had completed successfully.

## Tools and Skills Used

- **Skills.** `superpowers:systematic-debugging`, `lavra:lavra-review`, `vibin:repo-status`, `vibin:save-to-md`, `testing:claude-in-mobile`, and Android emulator testing guidance were used or triggered across the session.
- **Shell and Git.** Used `git`, `gh`, `cargo`, `gradle`, `adb`, Android SDK tools, `bd`, `rg`, and repository scripts for builds, tests, releases, and status evidence.
- **Labby MCP gateway.** Used `labby gateway list` and `labby gateway code exec` to access `claude-in-mobile` MCP tools for Android install/launch/device checks.
- **Android tooling.** Used `adb`, `emulator`, `keytool`, `zipalign`, `apksigner`, `logcat`, `uiautomator`, and `retrace`.
- **GitHub Actions and releases.** Used `gh run list`, `gh run watch`, `gh release view`, and `gh release download`.
- **Beads.** Used `bd list`, `bd show`, `bd search`, and `bd create` for tracker context and follow-up creation.

## Commands Executed

| command | result |
|---|---|
| `labby gateway list \| rg -i 'claude-in-mobile\|mobile'` | Confirmed `claude-in-mobile` upstream was connected. |
| `adb kill-server; adb -a start-server` | Rebound host ADB so gateway/container clients could reach port 5037. |
| `/home/jmagar/Android/Sdk/emulator/emulator -avd axon_crash_debug -no-window -no-audio -no-boot-anim -no-snapshot -gpu off ...` | Booted a headless emulator for APK testing. |
| `labby gateway code exec --json --code 'async () => await codemode.claude_in_mobile.app({ action: "install", path: "/tmp/axon-android-release-v1.3/axon-android-1.3.apk" })'` | Installed the GitHub release APK through MCP. |
| `labby gateway code exec --json --code 'async () => await codemode.claude_in_mobile.app({ action: "launch", package: "com.axon.app" })'` | Launched the published Android v1.3 APK. |
| `adb -s emulator-5554 logcat -d -b crash` | Crash buffer was empty for Android v1.3. |
| `./gradlew :app:assembleRelease --no-daemon` | Built minified Android release APK locally. |
| `git push origin main && git tag -a android-v1.3 -m "Android v1.3" && git push origin android-v1.3` | Published Android fix and release tag. |
| `gh run watch 27487451669 --repo jmagar/axon --interval 10 --exit-status` | Android release workflow completed successfully. |
| `gh release download android-v1.3 --repo jmagar/axon --pattern 'axon-android-1.3.apk*' --dir /tmp/axon-android-release-v1.3` | Downloaded the published APK and checksum. |
| `sha256sum -c axon-android-1.3.apk.sha256` | Verified release APK checksum. |
| `git remote prune origin` and `git worktree prune --verbose` | Safe cleanup commands ran as no-ops. |
| `bd create --title "Resolve PR #212 spider examples conflicts" ...` | Created follow-up bead `axon_rust-l4dd`. |

## Errors Encountered

- **Initial Android test gap.** The agent initially treated missing emulator/device state as a blocker. The user corrected this, and Labby `claude-in-mobile` was used.
- **ADB bridge failure.** `claude-in-mobile system info` hit `failed to connect to '172.19.0.1:5037': Connection refused`; rebinding host ADB with `adb -a start-server` fixed gateway access.
- **No Android target.** Labby initially listed only Browser; booting a headless AVD made `emulator-5554` visible.
- **Android v1.2 crash.** Published v1.2 installed but crashed on launch. Retrace showed Navigation Compose typed route serializer lookup failing after R8 minification.
- **Labby code-mode introspection mistake.** A direct `tools.filter(...)` call inside `execute` failed because `tools` was not defined there; device/app helper calls were used instead.
- **GitHub release watch was quiet during assemble.** `gh run watch` showed a long quiet assemble step; the job later completed successfully.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Ask synthesis context | GPT/Gemini/Claude-class models could receive only one full doc or near-duplicate docs, producing incomplete answers. | Higher-capacity models receive broader full-doc context and source use is tightened. |
| Codex LLM backend | Codex app-server backend was orphaned/not usable as a configured synthesis backend. | PR #213 branch wires Codex app-server backend through config, model selection, dispatch, job snapshots, and tests. |
| Android release | v1.2 release APK crashed on launch. | v1.3 release APK installs and launches cleanly on emulator. |
| Mobile testing skill | Agent could stop at "no device/emulator attached". | Lab skill explains Labby, host ADB bridge, AVD creation/boot, and retry flow. |
| Release updater | Axon had no observed merged release binary updater on `main` at earlier snapshot. | `main` now includes `feat(cli): add release binary updater (#211)` at `e897be76`. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `./gradlew :app:assembleRelease --no-daemon` | Minified Android release builds. | Build succeeded. | pass |
| `adb -s emulator-5554 shell pidof -s com.axon.app` after local minified install | App process remains alive. | Process ID returned. | pass |
| `adb -s emulator-5554 logcat -d -b crash` after local minified install | No crash. | `crash_lines=0`. | pass |
| `adb -s emulator-5554 shell dumpsys package com.axon.app \| rg 'versionCode\|versionName'` | Android v1.3 metadata. | `versionCode=4`, `versionName=1.3`. | pass |
| `gh run watch 27487451669 --repo jmagar/axon --exit-status` | Android release workflow succeeds. | Workflow `android-release` completed successfully. | pass |
| `sha256sum -c axon-android-1.3.apk.sha256` | Published checksum matches APK. | `axon-android-1.3.apk: OK`. | pass |
| Labby `claude_in_mobile.app install/launch` on downloaded v1.3 APK | Install and launch succeed. | `Performing Streamed Install\nSuccess` and `Launched com.axon.app/.MainActivity`. | pass |
| `adb devices` after emulator shutdown | No emulator left running. | Device list empty. | pass |
| `gh run list --repo jmagar/axon --limit 4` during save pass | Current CI state known. | Docker image and auto-tag success; `main` CI in progress. | warn |

## Risks and Rollback

- Android keep rules are narrowly scoped to `com.axon.app.ui.nav.**Route`; rollback is reverting commit `5c028b88` and Android tag `android-v1.3`, but that would reintroduce the release launch crash.
- PR #213 worktree has dirty changes after its saved session log; do not remove or overwrite it without reviewing those edits.
- `codex/fix-source-doc-char-boundary` is merged but dirty; deleting it would lose uncommitted source/test changes.
- Lab skill changes were pushed to the existing Lab branch `codex/snippets-cli-mcp`; rollback is reverting Lab commit `4defbcee` if that branch should not carry testing-skill docs.

## Decisions Not Taken

- Did not delete `codex/fix-source-doc-char-boundary` because it has uncommitted edits.
- Did not delete `codex/codex-app-server-llm-backend` because it has an open PR and dirty edits.
- Did not move the untracked Codex app-server plan into completed plans because it is active/ambiguous in `main`.
- Did not close PR #212; created a bead instead because it is conflicting and needs intentional review.
- Did not force-push or rewrite any branch.

## References

- Android v1.3 release: https://github.com/jmagar/axon/releases/tag/android-v1.3
- Android release workflow run: https://github.com/jmagar/axon/actions/runs/27487451669
- PR #211 release updater: https://github.com/jmagar/axon/pull/211
- PR #212 spider examples: https://github.com/jmagar/axon/pull/212
- PR #213 Codex app-server backend: https://github.com/jmagar/axon/pull/213
- Lab `claude-in-mobile` skill commit: `4defbcee docs(testing): document mobile emulator recovery`
- Axon Android fix commit: `5c028b88 fix(android): keep typed nav route serializers`

## Open Questions

- PR #212 needs a decision: resolve conflicts and continue, or close as superseded.
- PR #213 was open with mergeability `UNKNOWN` during this save pass and its worktree had dirty edits; it needs a fresh status review before cleanup or merge.
- The untracked `docs/superpowers/plans/2026-06-14-codex-app-server-llm-backend.md` in `main` should be reconciled with PR #213 or deliberately removed after review.
- The `.claude/current-plan` value points to `/home/jmagar/workspace/axon_rust/...`, which may be stale relative to the current `axon` repo.

## Next Steps

- Check current `main` CI run `27496520565` and wait for completion before declaring `main` fully green after PR #211.
- Review PR #213 and dirty `codex/codex-app-server-llm-backend` worktree before merging, pushing additional fixes, or cleanup.
- Work bead `axon_rust-l4dd` to resolve or close PR #212.
- Review the dirty `codex/fix-source-doc-char-boundary` worktree and either preserve, commit, or intentionally discard those uncommitted changes after ownership is clear.
- Reconcile the untracked Codex app-server plan file on `main`.
