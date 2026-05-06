# Pre-lite-mode archive

These docs describe an earlier multi-backend architecture (Postgres + AMQP/lapin + Redis) and a richer web UI (Pulse chat, full REST API, WebSocket protocol, supervisor, hot-reload, etc.) that have since been removed.

They are kept here for historical reference. **Do not treat their claims as accurate for the current codebase.** axon is now SQLite-only with in-process workers and an admin-only web panel.

Archived 2026-05-06 as part of the comprehensive stale-docs audit. See `docs/reports/2026-05-06-stale-docs-audit/00-MASTER-REPORT.md` for context.

| File | Reason archived |
|------|-----------------|
| `API.md` | REST API surface gutted; only admin endpoints remain |
| `CLAUDE-HOT-RELOAD.md` | UI feature for the removed Pulse surface |
| `MIGRATIONS.md` | Postgres-era migration notes; no longer applicable |
| `RESTORE.md` | Multi-backend restore flow; lite-mode has none |
| `SCALING.md` | Multi-node scaling notes; axon is single-node |
| `SERVE.md` | Documented a fictional supervisor that was never built |
| `UI-DESIGN-SYSTEM.md` | Pulse-era design tokens; admin panel uses different system |
| `WEB-ARCHITECTURE.md` | Described removed Next.js Pulse app and `/app/api/` routes |
| `WS-PROTOCOL.md` | WebSocket protocol for the removed real-time UI |
