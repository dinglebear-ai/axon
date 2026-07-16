# Pruning
Last Modified: 2026-07-15

Pruning handles scoped cleanup, duplicate detection/removal policy, targeted
removal, and cleanup-debt execution.

## Policy

Top-level `purge` and `dedupe` commands are removed. Cleanup behavior belongs
under prune plans and prune execution; those names are not public prune
subactions.

## Safety

Prune must default to dry-run planning. Destructive execution requires explicit
confirmation and admin policy. Results must report deleted counts by target
type and any remaining cleanup debt.
