//! Removed API DTO name registry used by schema-contract generation's
//! removed-surface drift check.
//!
//! **History (2026-07-12 alignment audit, #298 finding "axon-api
//! schema_registry.rs is a disconnected placeholder"):** this module used to
//! also define `DtoSchemaSpec`/`dto_schema_registry()`/`enum_schema_registry()`
//! — a hand-maintained registry of invented DTO/enum names (`SourceRecord`,
//! `LedgerEntry`, `ResetPlan`, ...) that never matched any real generated
//! `$defs` entry and had zero real callers outside a self-referential test
//! (the registry asserting it contained its own hardcoded family strings).
//! Those were deleted as dead/fictional. The **real** required/deferred API
//! DTO name registry — the one that actually gates
//! `docs/reference/api/schemas.json` generation — is
//! `xtask/src/schemas/api_defs.rs`'s `PHASE_1_REQUIRED_API_DEFS` /
//! `PHASE_1_DEFERRED_API_DEFS`, which is xtask-local (not exposed from
//! `axon-api`) because it derives schemas straight from the real
//! `axon_api::source::*` types via `schemars::JsonSchema`. See
//! `docs/pipeline-unification/schemas/api-dto-schema.md`'s "DTO Registry
//! Source" section for the up-to-date description.
//!
//! [`removed_dto_names`] below is the one function from the old module that
//! *is* real and load-bearing: `xtask/src/schemas/registry.rs`'s
//! `check_removed_api_dto_shapes` (called from `check_removed_surface_drift`,
//! exercised by `xtask/src/schemas/tests.rs`) asserts none of these names
//! reappear as `$defs` entries in the generated API schema.

pub fn removed_dto_names() -> &'static [&'static str] {
    &[
        "EmbedRequest",
        "IngestRequest",
        "CrawlRequest",
        "ScrapeRequest",
        "CodeSearchRequest",
    ]
}
