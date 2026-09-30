# Reset

Last reviewed: 2026-09-29

Reset is a destructive clean-slate operation for selected Axon stores. It is
separate from source pruning, job retention, and schema migration.
The [CLI wrapper](../../../crates/axon-cli/src/commands/reset.rs) delegates to
the shared reset service and defaults to a reviewable dry-run plan.

## Review the target first

On the intended runtime host, verify the binary version, effective config and
data paths, selected collections, and authorization scope. Make and validate
a [backup](backup-restore.md) before a destructive reset. Do not assume
`--collection` or a local working directory limits every store in the plan.
Read the reported stores, locations, counts, warnings, and `plan_id`.

```bash
# Plan only. No reset subcommand is required.
axon reset --json
```

The current CLI is flag-based, not `reset plan` / `reset exec`. Plans may
persist their own review metadata; dry-run means the selected stores are not
reset, not that no bookkeeping write can occur.

## Execute a reviewed plan

After explicit approval of the target and its effects, execute the returned
plan ID with the same intended configuration and scope:

```bash
# Destructive. Replace the placeholder with the reviewed plan identifier.
axon reset --yes --plan-id <plan_id> --json
```

Do not substitute `--confirm`, omit the reviewed identifier, or replay a plan
after changing its target. Remote callers must satisfy the transport's
authorization and confirmation contract; the trusted-local CLI shortcut is
not permission to run a remote reset. Use the discovered REST/MCP schema
rather than translating CLI flags into guessed fields.

## Verify the result

Retain the reset receipt and inspect per-store deleted/created counts,
warnings, and rebuilt schema/collections. Reconnect the intended runtime and
check readiness and read-only inventory. An accepted request, empty list, or
process exit does not by itself demonstrate a complete reset.

On failure, preserve the plan, receipt, logs, and remaining store state.
Report partial work and unknown commit status; do not blindly rerun a
destructive command or manually delete the remaining database files. Follow
the named recovery action or restore the validated recovery point.
