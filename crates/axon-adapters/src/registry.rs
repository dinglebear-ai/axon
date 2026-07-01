//! Source adapter registry.

use std::sync::Arc;

use axon_api::source::*;

use crate::adapter::SourceAdapter;

#[derive(Clone, Default)]
pub struct SourceAdapterRegistry {
    adapters: Vec<Arc<dyn SourceAdapter>>,
}

impl SourceAdapterRegistry {
    pub fn from_adapters<A>(adapters: Vec<A>) -> Self
    where
        A: SourceAdapter + 'static,
    {
        Self::from_arc_adapters(
            adapters
                .into_iter()
                .map(|adapter| Arc::new(adapter) as Arc<dyn SourceAdapter>)
                .collect(),
        )
    }

    pub fn from_arc_adapters(mut adapters: Vec<Arc<dyn SourceAdapter>>) -> Self {
        adapters.sort_by(|left, right| left.name().cmp(right.name()));
        Self { adapters }
    }

    pub fn from_boxed_adapters(adapters: Vec<Box<dyn SourceAdapter>>) -> Self {
        Self::from_arc_adapters(adapters.into_iter().map(Arc::from).collect())
    }

    pub fn adapter_for(&self, route: &RoutePlan) -> Option<Arc<dyn SourceAdapter>> {
        self.adapters
            .iter()
            .find(|adapter| adapter.name() == route.adapter.name)
            .cloned()
    }
}
