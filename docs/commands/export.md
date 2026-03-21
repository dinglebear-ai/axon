# export
Last Modified: 2026-03-21

Export backup manifest for rebuilding Axon state.

## Usage

```bash
# Seed-only backup (default)
axon export --output .cache/axon-rust/output/backup.json

# Include historical job sections
axon export --include-history --output .cache/axon-rust/output/backup-full.json

# Verify backup integrity/contract before restore
axon export verify .cache/axon-rust/output/backup.json
```

## Behavior

- Default mode is seed-only (`--include-history` disabled).
- Seed-only includes rebuild seeds, settings snapshot, integrity, refresh schedules, and watches.
- Seed-only omits history-heavy job sections by default.
- `--include-history` enables historical sections (`crawls`, `scrapes`, `extractions`, `embeds`, `ingests`, `refreshes.jobs`).

## Related

- [Export Backup Contract](../EXPORT.md)
