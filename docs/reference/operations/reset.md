# Reset
Last Modified: 2026-07-15

Reset is the destructive clean-slate operation for local Axon state.

## Policy

Reset is separate from prune. Reset clears local runtime stores, caches, and
artifacts according to a reviewed plan. It must default to dry-run behavior and
require explicit confirmation before mutation.

## Target State

The final reset path must not preserve or special-case legacy source-family job
tables. Old `axon_*_jobs` family tables are removal targets, not compatibility
surfaces.
