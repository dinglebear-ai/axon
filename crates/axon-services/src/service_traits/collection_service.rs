//! `CollectionService` — Qdrant collection discovery/lifecycle (list/get/
//! ensure/delete).
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §CollectionService. `crate::system::collections::collections` returns only
//! a `Vec<String>` of collection names — no per-collection vector/sparse
//! config. `list` wraps it anyway, mapping each name into a placeholder
//! `CollectionSpec` (a synthesized single-dimension `dense` `VectorConfig`
//! since the DTO's `dense` field is mandatory, no `sparse`, no
//! `payload_indexes`) since no richer per-collection metadata is available
//! from this call. `get`/`ensure`/`delete` have no backing free function and
//! remain stubs. The `Fake` implements real (in-memory) semantics using the
//! already-real `axon_api::source::vector::CollectionSpec` DTO.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::source::DeleteResult;
use axon_api::source::vector::CollectionSpec;

use crate::context::ServiceContext;
use crate::service_traits::not_implemented;

#[async_trait]
pub trait CollectionService: Send + Sync {
    async fn list(&self) -> anyhow::Result<Vec<CollectionSpec>>;
    async fn get(&self, collection: String) -> anyhow::Result<CollectionSpec>;
    async fn ensure(&self, spec: CollectionSpec) -> anyhow::Result<CollectionSpec>;
    async fn delete(&self, collection: String) -> anyhow::Result<DeleteResult>;
}

pub struct CollectionServiceImpl {
    ctx: Arc<ServiceContext>,
}

impl CollectionServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

/// Build a placeholder `CollectionSpec` for a bare collection name returned
/// by `crate::system::collections::collections`, which carries no
/// per-collection vector/sparse metadata. `dense` is a synthesized
/// single-dimension config since the DTO's `dense` field is mandatory.
fn placeholder_spec(name: String) -> CollectionSpec {
    CollectionSpec {
        collection: name,
        dense: axon_api::source::vector::VectorConfig {
            name: "dense".to_string(),
            dimensions: 1,
            distance: axon_api::source::vector::VectorDistance::Cosine,
        },
        payload_indexes: Vec::new(),
        sparse: None,
        aliases: Vec::new(),
        distance: None,
        metadata: axon_api::source::MetadataMap::new(),
    }
}

#[async_trait]
impl CollectionService for CollectionServiceImpl {
    async fn list(&self) -> anyhow::Result<Vec<CollectionSpec>> {
        let result = crate::system::collections(self.ctx.cfg.as_ref())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(result
            .collections
            .into_iter()
            .map(placeholder_spec)
            .collect())
    }

    async fn get(&self, _collection: String) -> anyhow::Result<CollectionSpec> {
        Err(not_implemented("CollectionService::get"))
    }

    async fn ensure(&self, _spec: CollectionSpec) -> anyhow::Result<CollectionSpec> {
        Err(not_implemented("CollectionService::ensure"))
    }

    async fn delete(&self, _collection: String) -> anyhow::Result<DeleteResult> {
        Err(not_implemented("CollectionService::delete"))
    }
}

#[cfg(test)]
fn fake_spec(name: &str) -> CollectionSpec {
    CollectionSpec {
        collection: name.to_string(),
        dense: axon_api::source::vector::VectorConfig {
            name: "dense".to_string(),
            dimensions: 1024,
            distance: axon_api::source::vector::VectorDistance::Cosine,
        },
        payload_indexes: Vec::new(),
        sparse: None,
        aliases: Vec::new(),
        distance: None,
        metadata: axon_api::source::MetadataMap::new(),
    }
}

/// Deterministic in-memory fake covering every `CollectionService` method.
#[derive(Default)]
pub struct FakeCollectionService {
    collections: Mutex<std::collections::HashMap<String, CollectionSpec>>,
}

impl FakeCollectionService {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CollectionService for FakeCollectionService {
    async fn list(&self) -> anyhow::Result<Vec<CollectionSpec>> {
        Ok(self.collections.lock().unwrap().values().cloned().collect())
    }

    async fn get(&self, collection: String) -> anyhow::Result<CollectionSpec> {
        self.collections
            .lock()
            .unwrap()
            .get(&collection)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("collection {collection} not found"))
    }

    async fn ensure(&self, spec: CollectionSpec) -> anyhow::Result<CollectionSpec> {
        let mut collections = self.collections.lock().unwrap();
        collections
            .entry(spec.collection.clone())
            .or_insert(spec.clone());
        Ok(collections.get(&spec.collection).cloned().unwrap())
    }

    async fn delete(&self, collection: String) -> anyhow::Result<DeleteResult> {
        let removed = self
            .collections
            .lock()
            .unwrap()
            .remove(&collection)
            .is_some();
        Ok(DeleteResult {
            deleted: removed,
            id: collection,
        })
    }
}

#[cfg(test)]
#[path = "collection_service_tests.rs"]
mod tests;
