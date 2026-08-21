//! External-caller smoke for adapter/provider boundaries.

use std::sync::Arc;

use axon_adapters::boundary::{FakeAdapterProviders, FetchProvider, RenderProvider};
use axon_adapters::{SourceAdapter, web::WebSourceAdapter};

#[test]
fn web_adapter_is_constructible_only_from_public_boundaries() {
    let providers = Arc::new(FakeAdapterProviders::new());
    let fetch: Arc<dyn FetchProvider> = providers.clone();
    let render: Arc<dyn RenderProvider> = providers;
    let adapter = WebSourceAdapter::new(fetch, render);
    assert_eq!(adapter.name(), "web");
    assert!(!adapter.version().is_empty());
}
