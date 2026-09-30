---
title: "Desktop Palette Testing"
created: 2026-05-18
updated: 2026-09-29
---

# Desktop Palette Testing

Last reviewed: 2026-09-29

Validate the Palette frontend separately from its native shell and live Axon
API connection. The app is in [apps/palette-tauri](../../apps/palette-tauri/),
with its own package manifest and standalone Tauri Cargo workspace. A browser
fixture passing does not prove native window, credential, or network behavior.

## Frontend checks and deterministic fixtures

From the Axon repository root:

```bash
pnpm --dir apps/palette-tauri install --frozen-lockfile
pnpm --dir apps/palette-tauri lint
pnpm --dir apps/palette-tauri test
pnpm --dir apps/palette-tauri typecheck
pnpm --dir apps/palette-tauri vite:build

# Interactive fixture pages, each run separately
pnpm --dir apps/palette-tauri fixture:operation-results
pnpm --dir apps/palette-tauri fixture:labby-cortex
```

The fixture scripts and check commands are owned by
[package.json](../../apps/palette-tauri/package.json). Record the revision and
fixture URL when capturing results. Check long titles/URLs, empty results,
partial success, visible errors, keyboard navigation, focus, and narrow-window
layout. Fixture screenshots demonstrate presentation only, not a successful
real provider call.

For API changes, regenerate the matching OpenAPI/client artifacts and run
`cargo xtask check-openapi-drift` as described in
[documentation maintenance](documentation.md). Do not hand-edit generated
client types to make a test compile.

## Native Windows build and packaging

The Windows portable artifact is built by
[palette-release.yml](../../.github/workflows/palette-release.yml), independently
of the CLI release workflow. Its native Windows build uses:

```powershell
pnpm --dir apps/palette-tauri install --frozen-lockfile
pnpm --dir apps/palette-tauri exec tauri build --no-bundle --ci --config src-tauri/tauri.ci.conf.json
```

Run this on a Windows build host with the app's native build prerequisites.
The workflow packages
`apps/palette-tauri/src-tauri/target/release/axon-palette-tauri.exe` and emits
archive checksums. A different explicit target or Cargo target directory can
change the local output path; inspect the build result instead of assuming a
GNU cross-compilation path.

The current Windows portable archive contains the Palette executable. Do not
assume an adjacent `axon.exe` is bundled or that launching the client starts
a production Axon server. Configure and verify the intended API endpoint
separately. Use the installed app's own profile/connection workflow, and keep
API authorization distinct from web-panel unlock credentials.

## Isolated live smoke test

Use a dedicated test directory and an explicitly selected Windows desktop
session. Record hostname, user, working directory, binary path, revision, API
origin, and test data scope. The host name is an environment choice, not a
repository contract.

Provision only the credentials needed for the chosen test endpoint. Do not
copy an entire server `.env`, token store, or home directory into the test
profile. Avoid broad firewall changes or stopping all processes named `axon`.

A native launch that retains the test process ID is easier to clean up safely:

```powershell
$palette = Start-Process -FilePath C:\axon-test\portable\axon-palette-tauri.exe -PassThru
$palette.Id
# After collecting evidence, stop only this test instance when still running:
Get-Process -Id $palette.Id -ErrorAction SilentlyContinue | Stop-Process
```

Choose a test path that exists on the host. A process exit or successful
`Start-Process` is not a UI health check. Use an interactive desktop session
for keyboard, focus, native dialogs, and screenshots; a non-interactive SSH
session may not provide those capabilities. When using automation, discover
the connected device tool and verify its target rather than assuming a
particular machine or screenshot action name.

Exercise read-only status/query/retrieve first. Source submission, extraction,
and cancellation require a deliberately selected test corpus and the
appropriate authorization. Keep destructive reset/prune/clear operations out
of ordinary smoke tests unless separately planned and approved.

## Follow queued work through the unified lifecycle

A source result or job ID is not completion. For a source submitted from the
Palette, inspect the same durable job through the UI or its API. On a CLI host
connected to the same local runtime state, use:

```bash
axon jobs get <job_id> --json
axon jobs events <job_id> --json
```

Do not use retired crawl/ingest family status commands or infer a job's
documents by guessing an output directory. Source identity, generations,
manifest entries, job artifacts, and counts are provided by the shared
contracts. Follow opaque artifact IDs rather than exposing server paths.
See [jobs](../reference/runtime/jobs.md),
[ledger](../reference/runtime/ledger.md), and
[Palette surface](../reference/surfaces/palette.md).

## Acceptance evidence

Record frontend check results, native build/revision, fixture captures, and
live API/job verification separately. Every operation must show a result or
a diagnostic that identifies the failure and recovery action. Distinguish
queued, running, completed, degraded, failed, and canceled states; retain the
job ID for follow-up.

Verify that result rows, provenance, long URLs, and errors remain readable;
keyboard focus and navigation remain usable; and successful completion is not
rendered as a failure. Capture native dialogs with a desktop screenshot when
necessary, with credentials and private content redacted. Report unavailable
providers or desktop automation explicitly instead of treating an untested
path as passed.
