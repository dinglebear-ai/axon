# axon-extract Agent Instructions

This file is the agent-facing contract for the `axon-extract` crate docs.

## When Editing

- Keep vertical extractor implementations (`INFO`/`matches()`/`extract()` per
  site module), `VerticalContext`, `VerticalError`, `ScrapedDoc`, and
  `ExtractorInfo` here.
- Do not add dispatch/routing logic, a dependency on `axon-adapters`, ledger
  persistence, chunking, embedding, or vector writes — dispatch order lives in
  `axon-adapters::vertical_registry`, and the dependency direction is
  one-way (`axon-adapters -> axon-extract`).
- New extractors follow the two-repo steps in
  `crates/axon-extract/src/CLAUDE.md`'s "Adding a New Extractor" section: add
  the module here, then wire `list()`/`dispatch_by_url()`/`dispatch_by_name()`
  in `axon-adapters`.
- Update `README.md` here and
  `../../foundation/crate-structure.md` together when the module surface or
  dependency direction changes.
- This crate is a documented exception to the clean-break removal list: it was
  restored to workspace `members` after being marked for removal, and
  `../../plans/finish-unification-metaplan.md` still frames it as transitional
  pending re-homing into adapter/parser ownership. Do not treat that
  transitional framing as license to skip contract upkeep — keep this contract
  and `crate-structure.md` synced to the real crate the same as any other
  member.

## Review Checklist

- Every extractor module exposes `INFO`, `matches()`, and `extract()` with no
  trait objects.
- `axon-extract`'s `[dependencies]` never adds `axon-adapters`.
- New/changed extractors keep a `matches()` truth-table test and, if
  `auto_dispatch: true`, get an exhaustiveness-test-covered
  `dispatch_by_url()`/`dispatch_by_name()` arm in `axon-adapters`.
- `ScrapedDoc.extractor_version` is bumped when an extractor's output shape
  changes, since it drives reindex-on-upgrade behavior.
