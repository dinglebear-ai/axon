# Desktop Palette Testing
Last Modified: 2026-06-14

This document covers how to validate the Tauri palette
(`apps/palette-tauri`, binary `axon-palette-tauri.exe`) on Windows.

## Capture Workflow

> **Note:** The `scripts/capture-palette-operations.ps1` script referenced in
> older versions of this document was never committed to the repository. Use the
> Windows-MCP approach below instead.

Palette testing runs on **agent-os** (Claude's Windows 11 VM on tootie,
reachable via `ssh agent-os`). Use the `vibin:desktop-app-testing` skill or
drive agent-os directly via the Windows-MCP tool available through Labby.

### Building a Windows binary

The portable Windows `.exe` is produced by `tauri build --no-bundle` and shipped
by the `palette-windows` job in `.github/workflows/release.yml`. To build one
locally for testing, use `scripts/build-on-steamy.sh` / `scripts/build-windows.sh`
(both default to `--target palette-tauri`), or build directly:

```bash
pnpm --dir apps/palette-tauri install --frozen-lockfile
pnpm --dir apps/palette-tauri exec tauri build \
  --target x86_64-pc-windows-gnu --no-bundle --ci
# → apps/palette-tauri/src-tauri/target/x86_64-pc-windows-gnu/release/axon-palette-tauri.exe
```

### Prerequisites

1. Build or download a portable Windows palette directory containing:
   - `axon-palette-tauri.exe`
   - `axon.exe`

   The palette needs the WebView2 runtime, which ships with Windows 11.

2. Copy the runtime config to the Windows user:

```powershell
# Run from dookie
scp ~/.axon/.env agent-os:'C:/Users/User/.axon/.env'
scp ~/.axon/config.toml agent-os:'C:/Users/User/.axon/config.toml'
```

3. Copy the palette binaries to agent-os:

```bash
scp ./apps/palette-tauri/src-tauri/target/x86_64-pc-windows-gnu/release/axon-palette-tauri.exe \
    ./target/x86_64-pc-windows-gnu/release/axon.exe \
    agent-os:'C:/axon-test/portable/'
```

4. Unblock downloaded executables (if copied from Linux or downloaded):

```powershell
Get-ChildItem C:\axon-test\portable -Recurse -Include *.exe | Unblock-File
```

5. Accept or pre-create Windows Firewall rules for `axon.exe` if a network
   prompt appears on first launch.

### Running via Windows-MCP

Use Windows-MCP on agent-os to launch and interact with the palette. For each
operation to capture:

1. Kill any running palette process:

```powershell
Stop-Process -Name axon-palette-tauri,axon -Force -ErrorAction SilentlyContinue
```

2. Launch the palette:

```powershell
Start-Process C:\axon-test\portable\axon-palette-tauri.exe
Start-Sleep -Seconds 2
```

3. Send the operation input via Windows-MCP keyboard automation, then take a
   screenshot with the Windows-MCP `Screenshot` action. Repeat for each
   operation (`status`, `doctor`, `map`, `scrape`, `crawl`, `search`,
   `research`, `ask`, `ingest`, `ask-reset`).

4. Fetch screenshots back to Linux:

```bash
mkdir -p /tmp/axon-agent-os/final-captures
scp 'agent-os:C:/axon-test/captures/*.png' /tmp/axon-agent-os/final-captures/
```

### Notes

- Kill and relaunch the palette between operations so selected mode, input
  text, and output state do not leak between captures.
- Use a Windows-MCP full-desktop screenshot when a system dialog or other
  desktop-level issue needs to be captured.
- A plain SSH PowerShell session can start the app but may not foreground the
  palette window or deliver keyboard input reliably — use the Windows-MCP
  desktop automation path instead.

## Job ID Follow-Up

For queued palette operations, the first follow-up should be:

```bash
axon status
```

That gives the user the current queue view without forcing them to copy a UUID.
Power users can still inspect a specific job directly:

```bash
axon crawl status <job_id> --json
axon ingest status <job_id> --json
```

Current document lookup support by job id:

- Crawl jobs can be traced to filesystem artifacts. `crawl status <job_id>
  --json` includes `result_json.output_dir` / `output_path` after progress has
  been persisted; the associated documents are the `manifest.jsonl` entries and
  markdown files under that directory.
- Ingest jobs currently expose progress/count metadata, not a per-document
  manifest. Qdrant payloads include URL/source metadata, but not the originating
  ingest job id, so there is no first-class `axon ingest documents <job_id>`
  command today.

If we want job-id document browsing for all async work, add a first-class
document manifest keyed by job id or stamp embedded Qdrant points with
`job_id`, then expose it through `axon <kind> documents <job_id>`.

## Acceptance Criteria

- Every operation in the harness produces visible output or a visible error.
- Completed successful operations do not show an error-colored prompt state.
- Default output hides transport details, stdout/stderr labels, line counts, and
  raw CLI flag advice.
- URL-heavy results wrap cleanly and remain readable in a narrow window.
- Async job operations suggest `axon status`, not only a job id.
- Search/research output uses structured result rows instead of pasted terminal
  text.
