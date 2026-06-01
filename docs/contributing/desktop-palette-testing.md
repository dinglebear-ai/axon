# Desktop Palette Testing
Last Modified: 2026-05-18

This document covers the Windows screenshot harness for `axon-palette.exe` and
the current UX review checklist for operation output rendering.

## Capture Harness

Use [`scripts/capture-palette-operations.ps1`](../scripts/capture-palette-operations.ps1)
to launch the Windows palette, submit operations, capture PNGs, and write a
`manifest.json` beside the captures.

The script is intended to run on the Windows desktop session. A plain SSH
PowerShell session can start the app, but may not foreground the GPUI window or
deliver keyboard input reliably. Use Windows-MCP PowerShell for unattended
desktop runs.

## Prerequisites

1. Download or build a portable Windows palette directory containing:
   - `axon-palette.exe`
   - `axon.exe`
2. Copy the local Axon runtime config to the Windows user:
   - `~/.axon/.env`
   - `~/.axon/config.toml`
3. Unblock downloaded executables and scripts:

```powershell
Get-ChildItem C:\axon-test\portable -Recurse -Include *.exe,*.ps1 | Unblock-File
```

4. Accept or pre-create Windows Firewall rules for `axon.exe` if a network
   prompt appears. The first child process that binds or connects can block the
   palette until this is handled.

## Copy And Run

Copy the harness to the Windows target:

```bash
scp scripts/capture-palette-operations.ps1 \
  agent-os:'C:/axon-test/portable-56a2b8c4/capture-palette-operations.ps1'
```

Run a small smoke first:

```powershell
Stop-Process -Name axon-palette,axon -Force -ErrorAction SilentlyContinue
powershell -NoProfile -ExecutionPolicy Bypass `
  -File C:\axon-test\portable-56a2b8c4\capture-palette-operations.ps1 `
  -PalettePath C:\axon-test\portable-56a2b8c4\axon-palette.exe `
  -OutputDir C:\axon-test\portable-56a2b8c4\operation-screens-smoke `
  -Only status,doctor
```

Run the full set in chunks to avoid the Windows-MCP call timeout:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File C:\axon-test\portable-56a2b8c4\capture-palette-operations.ps1 `
  -PalettePath C:\axon-test\portable-56a2b8c4\axon-palette.exe `
  -OutputDir C:\axon-test\portable-56a2b8c4\operation-screens-a `
  -Only status,doctor,map,scrape

powershell -NoProfile -ExecutionPolicy Bypass `
  -File C:\axon-test\portable-56a2b8c4\capture-palette-operations.ps1 `
  -PalettePath C:\axon-test\portable-56a2b8c4\axon-palette.exe `
  -OutputDir C:\axon-test\portable-56a2b8c4\operation-screens-b `
  -Only crawl,search,research,ask

powershell -NoProfile -ExecutionPolicy Bypass `
  -File C:\axon-test\portable-56a2b8c4\capture-palette-operations.ps1 `
  -PalettePath C:\axon-test\portable-56a2b8c4\axon-palette.exe `
  -OutputDir C:\axon-test\portable-56a2b8c4\operation-screens-c `
  -Only ingest,ask-reset
```

Fetch captures back to Linux:

```bash
mkdir -p /tmp/axon-agent-os/final-captures
scp agent-os:'C:/axon-test/portable-56a2b8c4/operation-screens-a/*.png' /tmp/axon-agent-os/final-captures/
scp agent-os:'C:/axon-test/portable-56a2b8c4/operation-screens-b/*.png' /tmp/axon-agent-os/final-captures/
scp agent-os:'C:/axon-test/portable-56a2b8c4/operation-screens-c/*.png' /tmp/axon-agent-os/final-captures/
```

## Harness Notes

- `-Only` accepts operation names or numbered names, with comma-separated or
  space-separated values: `-Only status,map` or `-Only status map`.
- The harness kills and relaunches the palette for each operation so selected
  mode, input text, and output state do not leak between captures.
- Window-only screenshots are default. Pass `-FullScreen` when a system dialog
  or other desktop-level problem needs to be captured.
- If Windows-MCP `Screenshot` or `Snapshot` fails because the guest Python
  environment is missing `cv2`, use the script's built-in PowerShell screenshot
  path instead.

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

## Current Screenshot Review

Reviewed captures from:

- Windows target: `C:\axon-test\portable-56a2b8c4\operation-screens-final-*`
- Local copy: `/tmp/axon-agent-os/final-captures`

### High Priority

1. `ask` renders blank output.

   Screenshot: `08-ask.png`

   The window collapses to the prompt-only height after submitting
   `ask what is axon`. The direct CLI reports a missing `TEI_URL`, so the
   palette should show a concise error if the operation cannot run. Blank output
   makes the app look broken and gives no recovery path.

2. `ask-reset` renders blank output.

   Screenshot: `10-ask-reset.png`

   Reset should produce explicit feedback such as `Conversation reset`, even if
   there is no command payload to show. A no-op visual result is indistinguishable
   from a failed submit.

3. Successful operations still show an alarming red prompt status dot.

   Screenshots: `03-map.png`, `04-scrape.png`, `05-crawl.png`, `06-search.png`,
   `07-research.png`, `09-ingest.png`

   These screens say `completed`, but the prompt indicator is red. The status
   color should match the operation state or be removed from the prompt area.

### Output Formatting

4. `status` is too dense and wraps URLs poorly.

   Screenshot: `01-status.png`

   The URL list is much better than raw stderr, but long URLs wrap into hard to
   scan fragments. Job status should render as rows with separate state, URL,
   and job id fields, plus copy/open affordances where useful.

5. `doctor` still exposes raw diagnostic internals by default.

   Screenshot: `02-doctor.png`

   The output includes raw transport text like `error sending request for url`
   and internal endpoints. The default view should summarize service status in
   human labels (`TEI unreachable`, `Qdrant unreachable`, `Chrome unreachable`)
   and keep raw endpoints/details behind an expandable diagnostics view.

6. `map` shows CLI options and CLI flag advice in normal output.

   Screenshot: `03-map.png`

   `Options: maxDepth/discoverSitemaps` and `consider --map-fallback crawl` feel
   like terminal output. The desktop result should prioritize `No URLs found for
   https://example.com` and present fallback actions as buttons or secondary
   hints, not raw flags.

7. `search` and `research` still render like copied terminal text.

   Screenshots: `06-search.png`, `07-research.png`

   Results should be structured cards or compact rows with title, URL, and
   snippet. Current output contains odd `$` characters in snippets, inconsistent
   spacing, and text runs that are hard to scan.

8. `research` leaks internal provider/model metadata.

   Screenshot: `07-research.png`

   `provider=tavily model=` and `Search Results: 10 Pages Extracted: 10` should
   either be hidden or moved into metadata. The normal output should start with
   the answer/results the user asked for.

9. `research` text is clipped at the right edge.

   Screenshot: `07-research.png`

   Long result lines run past the visible panel instead of wrapping cleanly
   inside the output container.

### Operation Feedback

10. `crawl` and `ingest` should make `axon status` the next action.

    Screenshots: `05-crawl.png`, `09-ingest.png`

    Queued async operations should explicitly suggest `axon status` so the user
    has an obvious next step. Follow-up affordances can add copy job id, open
    errors, or cancel later.

11. Long result panels have no visible scroll affordance.

    Screenshots: `01-status.png`, `06-search.png`, `07-research.png`

    Content continues below the fold, but the output container does not make
    scrolling obvious. Add a visible scrollbar, fade, count, or footer hint.

12. `scrape` duplicates the page title.

    Screenshot: `04-scrape.png`

    `Example Domain` appears twice. The formatter should collapse duplicate
    headings extracted from the page title/body.

## Acceptance Criteria

- Every operation in the harness produces visible output or a visible error.
- Completed successful operations do not show an error-colored prompt state.
- Default output hides transport details, stdout/stderr labels, line counts, and
  raw CLI flag advice.
- URL-heavy results wrap cleanly and remain readable in a 736 px wide window.
- Async job operations suggest `axon status`, not only a job id.
- Search/research output uses structured desktop result rows instead of pasted
  terminal text.
