# axon doctor
Last Modified: 2026-03-03

Run connectivity and pipeline diagnostics for the local Axon stack.

## Synopsis

```bash
axon doctor [FLAGS]
```

## Flags

All global flags apply. Key flags for this command:

| Flag | Default | Description |
|------|---------|-------------|
| `--json` | `false` | Print full structured report JSON. |

## Checks Performed

`doctor` probes and reports:

For implementation details and troubleshooting see [`docs/ingest/doctor.md`](../ingest/doctor.md)

- Job pipeline readiness for `crawl`, `extract`, `embed`, `ingest`
- Service health for Qdrant, TEI, and optional Chrome endpoint
- Gemini headless command/config readiness for LLM-backed commands
- Browser runtime diagnostics settings
- Stale and pending job counts
- Probe timing metrics

### SQLite Job Runtime

The current runtime stores jobs in SQLite and runs workers in-process. The
doctor report includes:

- SQLite file presence (`exists`) and path
- TEI and Qdrant service probes
- Gemini headless readiness
- Chrome endpoint probe
- Browser runtime diagnostics
- compatibility fields such as `"lite_mode": true` in the report

## Examples

```bash
# Human-readable diagnostic report
axon doctor

# Full JSON report
axon doctor --json
```

## Notes

- Gemini CLI auth and `AXON_HEADLESS_GEMINI_*` settings control LLM readiness.
- Chrome is optional; report includes `configured` and probe status separately.
- `all_ok` focuses on core pipeline + TEI + Qdrant readiness.
