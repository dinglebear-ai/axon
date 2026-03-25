# export
Last Modified: 2026-03-25

Export backup manifest for rebuilding Axon state.

## Usage

```bash
# Seed-only backup — output file auto-named axon-export-{timestamp}.json in CWD
axon export

# Seed-only backup with explicit output path
axon export --output .cache/axon-rust/output/backup.json

# Include historical job sections
axon export --include-history --output .cache/axon-rust/output/backup-full.json

# Print manifest JSON to stdout instead of writing a file
axon export --json

# Verify backup integrity/contract before restore
axon export verify .cache/axon-rust/output/backup.json
```

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--include-history` | `false` | Include full job history sections. By default, export is seed-only. |
| `--output <path>` | auto-named | Output file path. Defaults to `axon-export-{timestamp}.json` in the current directory. |
| `--json` | `false` | Print the manifest JSON to stdout instead of writing a file. |

## Behavior

- Default mode is seed-only (`--include-history` disabled).
- Seed-only always includes: `rebuild_seeds`, `settings_snapshot`, `integrity`, `refresh_schedules` (`refreshes.schedules`), `watches`, and `qdrant_summary`.
- Seed-only omits history-heavy job sections: `crawls`, `scrapes`, `extractions`, `embeds`, `ingests`, `refreshes.jobs`.
- `--include-history` enables all omitted history sections.
- Without `--output` or `--json`, the manifest is written to `axon-export-{timestamp}.json` in the current working directory.
- With `--json`, the manifest is printed to stdout and no file is written.

## Related

- [Export Backup Contract](../EXPORT.md)
