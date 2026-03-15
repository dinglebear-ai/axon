# Jobs API Routes

Source of truth:
- [`app/api/jobs/route.ts`](../app/api/jobs/route.ts)
- [`app/api/jobs/[id]/route.ts`](../app/api/jobs/[id]/route.ts)
- [`lib/server/jobs.ts`](../lib/server/jobs.ts)

## Shared Helper Module

Shared mapping and filter helpers for the jobs routes live in `lib/server/jobs.ts`.

Helpers currently shared by both routes:

- `statusClause`
- `StatusFilter`
- `isoDateOrNull`
- `asJsonRecord`
- `stringOrNull`
- `numberOrNull`
- `boolOrNull`
- `stringArray`
- `summarizeUrls`
- `truncateJobTarget`
- `jobSuccessFromStatus`

## Route Responsibilities

### `GET /api/jobs`

Responsible for:

- validating `type` and `status` query filters
- building shared status filter SQL clauses
- listing jobs across job tables
- normalizing row output into the list response shape
- returning aggregate counts for the UI

### `GET /api/jobs/[id]`

Responsible for:

- locating a job by ID across job tables
- normalizing per-job detail payloads
- mapping `result_json` and `config_json` safely
- optionally returning crawl manifest artifacts

## Current Boundary

`lib/server/jobs.ts` is a shared helper module, not a full repository layer.

Current state:

- shared row coercion and display logic lives in the helper
- raw SQL still lives in the route handlers
- per-table lookup/query orchestration still lives in the route handlers

A future extraction can move the remaining SQL/data-access concerns into a dedicated repository layer without changing the route response contract.
