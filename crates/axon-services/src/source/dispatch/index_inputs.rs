//! Pure `*SourceIndexInput` construction for the five families whose acquire
//! step needs live network/subprocess access before indexing (`feed`,
//! `reddit`, `youtube`, `registry`, `session`).
//!
//! Split out of `dispatch.rs` so the `embed`/`max_items` assembly — the exact
//! seam issue #298 WS-D found silently dropping `SourceRequest.embed`/
//! `limits.max_items` (every one of these five `dispatch_*` functions
//! hardcoded `embed: true, max_items: None` regardless of what the caller
//! requested) — is unit-testable in isolation, without running the network
//! fetch / OAuth / subprocess acquire step each `dispatch_*` function performs
//! before calling into these builders. See `index_inputs_tests.rs`.

use std::path::PathBuf;

use axon_api::source::AuthSnapshot;

use crate::context::TargetLocalSourceRuntime;
use crate::{
    FeedSourceIndexInput, RedditSourceIndexInput, RegistrySourceIndexInput,
    SessionsSourceIndexInput, YoutubeSourceIndexInput,
};

use super::placeholder_job_id;

#[allow(clippy::too_many_arguments)]
pub(super) fn feed_index_input(
    runtime: &TargetLocalSourceRuntime,
    feed_path: PathBuf,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_items: Option<u64>,
) -> FeedSourceIndexInput {
    FeedSourceIndexInput {
        feed_path,
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
        auth_snapshot: auth_snapshot.cloned(),
        embed,
        max_items,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn reddit_index_input(
    runtime: &TargetLocalSourceRuntime,
    target: &str,
    dump_path: PathBuf,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_items: Option<u64>,
) -> RedditSourceIndexInput {
    RedditSourceIndexInput {
        target: target.to_string(),
        dump_path,
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
        auth_snapshot: auth_snapshot.cloned(),
        embed,
        max_items,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn youtube_index_input(
    runtime: &TargetLocalSourceRuntime,
    target: &str,
    youtube_dump_path: PathBuf,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_items: Option<u64>,
) -> YoutubeSourceIndexInput {
    YoutubeSourceIndexInput {
        target: target.to_string(),
        youtube_dump_path,
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
        auth_snapshot: auth_snapshot.cloned(),
        embed,
        max_items,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn registry_index_input(
    runtime: &TargetLocalSourceRuntime,
    registry_dump_path: PathBuf,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_items: Option<u64>,
) -> RegistrySourceIndexInput {
    RegistrySourceIndexInput {
        registry_dump_path,
        include_all_versions: false,
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
        auth_snapshot: auth_snapshot.cloned(),
        embed,
        max_items,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn session_index_input(
    runtime: &TargetLocalSourceRuntime,
    sessions_root: PathBuf,
    provider: String,
    session_id: String,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_items: Option<u64>,
) -> SessionsSourceIndexInput {
    SessionsSourceIndexInput {
        sessions_root,
        provider,
        session_id,
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
        auth_snapshot: auth_snapshot.cloned(),
        embed,
        max_items,
    }
}

#[cfg(test)]
#[path = "index_inputs_tests.rs"]
mod tests;
