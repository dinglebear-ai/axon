# axon doctor
Last Modified: 2026-06-01

Run connectivity and pipeline diagnostics for the local Axon stack.

## Synopsis

```bash
axon doctor [FLAGS]
axon doctor diagnose [FLAGS]
```

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `diagnose` | Print doctor output plus an LLM-assisted diagnosis when a Gemini headless command is configured. (Equivalent to `axon debug`.) |

## Flags

All global flags apply. Key flags for this command:

| Flag | Default | Description |
|------|---------|-------------|
| `--json` | `false` | Print full structured report JSON. |

## Checks Performed

`doctor` probes and reports:

- Job pipeline readiness for `crawl`, `extract`, `embed`, `ingest`
- Service health for Qdrant, TEI, and optional Chrome endpoint
- Gemini headless command/config checks for LLM-backed commands; `axon ask` is the completion proof
- Browser runtime diagnostics settings
- Stale and pending job counts
- Probe timing metrics

### SQLite Job Runtime

The current runtime stores jobs in SQLite and runs workers in-process. The
doctor report includes:

- SQLite file presence (`exists`) and path
- TEI and Qdrant service probes
- Gemini headless CLI/config status
- Chrome endpoint probe
- Browser runtime diagnostics
- no legacy runtime-mode marker fields

## Examples

```bash
# Human-readable diagnostic report
axon doctor

# Full JSON report
axon doctor --json
```

## Notes

- Gemini CLI auth and `AXON_HEADLESS_GEMINI_*` settings control LLM completion, but `doctor` does not prove completion readiness; run `axon ask` for that smoke.
- Chrome is optional; report includes `configured` and probe status separately.
- `all_ok` focuses on core pipeline + TEI + Qdrant readiness.
