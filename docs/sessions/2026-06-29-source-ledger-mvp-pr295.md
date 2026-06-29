# SourceLedger MVP PR #295

Date: 2026-06-29
Branch: `codex/source-ledger-mvp`
PR: https://github.com/jmagar/axon/pull/295

## Summary

Implemented and repaired the SourceLedger MVP slice for Axon. The final scope is:

- Generic `axon-source-ledger` SQLite lifecycle store with sources, manifest rows, leases, generations, backoff/status, and cleanup debt.
- Sealed vector payload stamping for `source_id`, `source_kind`, `source_generation`, `source_item_key`, and `source_index_version`.
- Mutable generic Git ingest adapter using SourceLedger manifests, collection-scoped source IDs, credential-redacted identities/logs/errors, owner-guarded commits, generation aborts, and cleanup debt draining.
- Sync crawl manifest finalization using SourceLedger generations after successful embed/upsert, full-manifest commits, changed-only embed comparison, collection-scoped source IDs, and cleanup debt draining.
- `axon embed <path> --watch` foregrounds local code-index watch progress for Git checkouts/workspaces; the removed `code-search-watch` command remains a tombstone rather than a normal command.
- Documentation was corrected to avoid claiming plain local `axon embed /path` is SourceLedger-backed. Plain existing local paths still run inline; `embed --watch` delegates to the existing local code-index lifecycle.

## Review Repairs

Mandatory review findings addressed during closeout:

- Collection collision: git/crawl SourceLedger source IDs now include collection, with regression tests.
- Incremental crawl mismatch: crawl compares prepared docs against `changed=true` manifest keys while committing the full manifest.
- Cleanup debt: SourceLedger cleanup debt is now executed through a typed Qdrant delete helper and cleared only after successful deletion.
- Silent initial watch failure: initial `embed --watch` refresh now exits nonzero if freshness returns stale/warning.
- Credential leakage: generic git logs parse first and log only credential-free `web_url`; clone stderr is redacted before surfacing.
- Docs drift: README, CLAUDE.md, embed action docs, and the implementation plan now describe the actual local embed/watch split.

Remaining type-design hardening noted by review but not folded into this PR: replacing split-phase store APIs with a generation lease token/newtypes and moving cleanup selector typing fully into the ledger/API layer.

## Validation

Passed:

- `cargo fmt --check`
- `cargo xtask check-layering`
- `cargo xtask check-version-sync`
- `cargo xtask check-openapi-drift`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- Focused tests:
  - `cargo test -p axon-source-ledger`
  - `cargo test -p axon-ingest generic_git -- --nocapture`
  - `cargo test -p axon-services crawl_changed_manifest_keys_excludes_reused_pages -- --nocapture`
  - `cargo test -p axon-services code_search_watch -- --nocapture`
  - `cargo test -p axon-vector ledger_payload_stamps_authoritative_source_fields -- --nocapture`
  - `cargo test -p axon-cli embed_watch -- --nocapture`
  - `cargo test -p axon-services source_status_redacts_headers_and_local_paths -- --nocapture`

Review coverage:

- Lavra-style architecture/data/security review wave ran before repairs.
- Three code simplifier passes ran before this closeout repair wave.
- Official PR-review-toolkit roles ran before this closeout repair wave.
- Local PR-review-toolkit code reviewer, silent failure hunter, and type design analyzer ran after repairs and drove the final fixes above.
- Remaining local PR-review-toolkit roles could not be spawned because the thread hit the active subagent limit.
