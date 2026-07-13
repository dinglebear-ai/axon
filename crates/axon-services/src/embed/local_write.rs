//! Local-directory/file embed execution via the ledger-tracked
//! `local_source` pipeline (`axon-document` + `axon-embedding` +
//! `axon-vectors`), replacing the legacy `axon_vector::ops::embed_path_native*`
//! entry points (#298).
//!
//! Builds a throwaway [`TargetLocalSourceRuntime`] from [`Config`] per call —
//! this mirrors the (also throwaway, non-cached) client construction the
//! legacy `embed_path_native_with_progress` did internally, and the same
//! lazy-build pattern `SourceRunner` (`runtime/job_runners/source_runner.rs`)
//! uses for the `Source` unified job kind.

use std::path::PathBuf;
use std::sync::Arc;

use axon_api::source::{AuthSnapshot, JobId};
use axon_core::config::Config;
use uuid::Uuid;

use crate::context::TargetLocalSourceRuntime;
use crate::{LocalSourceIndexInput, LocalSourceIndexOutput, LocalSourceSelectionPolicy};

/// Run a local-path embed end to end: resolve an enqueue-only job runtime,
/// build the target local-source data plane, then index `input` (a local
/// directory or file) through `local_source::index_local_source_with_job`.
///
/// `pub(crate)` (not `pub(super)`) so `runtime::job_runners::EmbedRunner`
/// (the `JobKind::Embed` unified job executor) can share this same runtime
/// construction instead of duplicating it.
pub(crate) async fn embed_local_path(
    cfg: &Config,
    input: &str,
    source_type: Option<&str>,
) -> anyhow::Result<LocalSourceIndexOutput> {
    let cfg_arc = Arc::new(cfg.clone());
    let jobs = crate::runtime::resolve_runtime(Arc::clone(&cfg_arc))
        .await
        .map_err(|error| anyhow::anyhow!("failed to resolve job runtime: {error}"))?;
    let (Some(pool), Some(store)) = (jobs.sqlite_pool(), jobs.unified_job_store()) else {
        anyhow::bail!("local embed requires the SQLite job runtime (no pool/unified job store)");
    };
    let runtime = TargetLocalSourceRuntime::from_config(cfg, store, (*pool).clone())
        .await
        .map_err(|error| anyhow::anyhow!("failed to build local source runtime: {error}"))?;

    // `source_type` (kept for API-compatibility with the legacy signature)
    // has no analog in `LocalSourceIndexInput` — the local-source pipeline
    // always stamps `source_type = "local_code"` via `axon-adapters`' local
    // adapter metadata, regardless of the caller-supplied hint. See the
    // `crate::embed` module for why the legacy fine-grained per-call
    // `source_type` distinction is dropped here.
    let _ = source_type;

    let index_input = LocalSourceIndexInput {
        root: PathBuf::from(input),
        collection: cfg.collection.clone(),
        owner_id: "runtime".to_string(),
        job_id: JobId::new(Uuid::nil()),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        selection_policy: LocalSourceSelectionPolicy::Permissive,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
        auth_snapshot: None::<AuthSnapshot>,
        embed: true,
        route: None,
    };

    crate::index_local_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}
