# SourceLedger MVP PR #295

Date: 2026-06-29
Branch: `codex/source-ledger-mvp`
PR: https://github.com/jmagar/axon/pull/295

## Summary

Implemented and repaired the SourceLedger MVP slice for Axon. The final scope is:

- Generic `axon-source-ledger` SQLite lifecycle store with sources, manifest rows, leases, generations, backoff/status, and cleanup debt.
- Sealed vector payload stamping for `source_id`, `source_kind`, `source_generation`, `source_item_key`, and `source_index_version`.
- Mutable generic Git ingest adapter using SourceLedger manifests, collection-scoped source IDs, credential-redacted identities/logs/errors, owner-guarded commits, generation aborts, and cleanup debt draining.
- Sync crawl manifest finalization using SourceLedger generations after successful embed/upsert, changed-only generation commits that preserve unchanged item generations, collection-scoped opaque source IDs, and cleanup debt draining.
- `axon embed <path> --watch` foregrounds local code-index watch progress for Git checkouts/workspaces; the removed `code-search-watch` command remains a tombstone rather than a normal command.
- Documentation was corrected to avoid claiming plain local `axon embed /path` is SourceLedger-backed. Plain existing local paths still run inline; `embed --watch` delegates to the existing local code-index lifecycle.

## Review Repairs

Mandatory review findings addressed during closeout:

- Collection collision: git/crawl SourceLedger source IDs now include collection, with regression tests.
- Incremental crawl mismatch: crawl compares prepared docs against `changed=true` manifest keys, commits changed items through the delta path, and uses the full manifest for diff/live-key pruning.
- Cleanup debt: SourceLedger cleanup debt is now executed through a typed Qdrant delete helper and cleared only after successful deletion.
- Silent initial watch failure: initial `embed --watch` refresh now exits nonzero if freshness returns stale/warning.
- Credential leakage: generic git logs parse first and log only credential-free `web_url`; clone stderr is redacted before surfacing.
- Docs drift: README, CLAUDE.md, embed action docs, and the implementation plan now describe the actual local embed/watch split.
- Qdrant visibility: SourceLedger-owned vectors are written hidden (`source_committed=false`), search filters exclude hidden points, and successful generation finalization publishes the generation with `source_committed=true`.
- Lease coverage: git and crawl refreshes extend their SourceLedger lease while embed/upsert/finalize work is active, and failed cleanup records retry/error state.
- Publish recovery: git/crawl refreshes republish the last committed generation before diffing so a transient publish failure cannot leave unchanged docs hidden forever.
- Qdrant verification: generation publish verifies visible point counts, cleanup deletes verify stale selector counts are zero before clearing debt, and URL/full-doc retrieve paths also exclude uncommitted points.
- Watch retries: foreground code-index watch keeps failed dirty roots queued for retry instead of silently dropping them after a refresh error.
- Type hardening: cleanup selectors now keep fields private behind getters, and SourceLedger validates cleanup selector JSON against the debt row source/generation/item before storing it.
- Watch ergonomics: `embed --watch` performs initial refresh for both direct Git checkout roots and workspace directories with discovered child checkouts.

Follow-up type-design hardening noted by review but not folded into this PR: replacing split-phase store APIs with a generation lease token/newtypes and moving cleanup selector typing fully into the ledger/API layer.

## Migration Runbook

- Pre-deploy: run `cargo xtask check-sqlite-migrations` to verify migration order/checksums.
- Post-deploy SQL check:
  - `SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'axon_source_%' ORDER BY name;`
  - `PRAGMA foreign_key_check;`
  - `SELECT source_kind, COUNT(*) FROM axon_source_sources GROUP BY source_kind;`
- Rollback: stop Axon processes before restoring the previous SQLite database snapshot. Do not partially drop SourceLedger tables from a live database.

## Validation

Passed in this review-fix wave:

- `cargo fmt`
- `cargo check -p axon-source-ledger -p axon-vector -p axon-ingest -p axon-services -p axon-cli`
- `cargo xtask check-sqlite-migrations`
- Focused tests:
  - `cargo test -p axon-source-ledger`
  - `cargo test -p axon-ingest generic_git`
  - `cargo test -p axon-services qdrant_down_does_not_allocate_local_generation`
  - `cargo test -p axon-services crawl_changed_manifest_keys_excludes_reused_pages`
  - `cargo test -p axon-services crawl_embed_failure_does_not_commit_generation`
  - `cargo test -p axon-services crawl_source_identity_is_collection_scoped`
  - `cargo test -p axon-vector exclude_uncommitted_source_filter`
  - `cargo test -p axon-vector point_id`
  - `cargo test -p axon-vector publish_source_generation`
  - `cargo test -p axon-vector cleanup_selector_batch`
  - `cargo test -p axon-vector excludes_uncommitted_source_points`
  - `cargo test -p axon-vector ledger_payload_stamps_authoritative_source_fields`

Review coverage:

- Lavra-style architecture/data/security review wave ran before repairs.
- Three code simplifier passes ran before this closeout repair wave.
- Local PR-review-toolkit code reviewer, silent failure hunter, type design analyzer, test analyzer, code simplifier, and comment analyzer were dispatched after the Lavra repair wave.
