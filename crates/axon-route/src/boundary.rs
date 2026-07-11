//! Trait boundaries for the group-route pipeline stage (`SourceResolver`,
//! `SourceRouter`).
//!
//! These traits share a bare identifier with the concrete structs that
//! implement them (`crate::resolver::SourceResolver`,
//! `crate::router::SourceRouter`). Rust's type namespace forbids a trait and
//! a struct with the same name in one module, so the traits live here while
//! the concrete structs keep their existing modules and public names
//! unchanged. Because of the collision, `axon-route::lib` does NOT
//! re-export these traits at the crate root — reach them via
//! `axon_route::boundary::SourceResolver` / `axon_route::boundary::SourceRouter`.
//!
//! This keeps the round non-breaking: every existing caller of the concrete
//! structs' inherent methods keeps compiling untouched, and the new
//! async-trait-shaped methods become reachable only through `&dyn Trait` /
//! generic-bound dispatch.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::source::*;
use axon_error::ApiError;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait SourceResolver: Send + Sync {
    async fn resolve(&self, request: &SourceRequest) -> Result<ResolvedSource>;
    async fn capabilities(&self) -> Result<SourceResolverCapability>;
}

#[async_trait]
pub trait SourceRouter: Send + Sync {
    async fn route(&self, source: ResolvedSource, request: &SourceRequest) -> Result<RoutePlan>;
    async fn validate_options(&self, plan: &RoutePlan) -> Result<ValidatedOptions>;
    async fn capabilities(&self) -> Result<SourceRouterCapability>;
}

/// Deterministic outcome for [`FakeSourceResolver`] / [`FakeSourceRouter`].
#[derive(Debug, Clone)]
pub enum FakeSourceRouteMode<T> {
    Success(T),
    Failure(ApiError),
    Degraded(T),
}

#[derive(Debug, Clone)]
pub struct FakeSourceResolver {
    mode: FakeSourceRouteMode<ResolvedSource>,
    calls: Arc<Mutex<Vec<SourceRequest>>>,
    capability_override: Option<SourceResolverCapability>,
}

impl FakeSourceResolver {
    pub fn new(mode: FakeSourceRouteMode<ResolvedSource>) -> Self {
        Self {
            mode,
            calls: Arc::new(Mutex::new(Vec::new())),
            capability_override: None,
        }
    }

    pub fn with_capability_override(mut self, capability: SourceResolverCapability) -> Self {
        self.capability_override = Some(capability);
        self
    }

    pub async fn calls(&self) -> Vec<SourceRequest> {
        self.calls
            .lock()
            .expect("fake source resolver call log mutex poisoned")
            .clone()
    }

    fn record(&self, request: &SourceRequest) {
        self.calls
            .lock()
            .expect("fake source resolver call log mutex poisoned")
            .push(request.clone());
    }
}

#[async_trait]
impl SourceResolver for FakeSourceResolver {
    async fn resolve(&self, request: &SourceRequest) -> Result<ResolvedSource> {
        self.record(request);
        match &self.mode {
            FakeSourceRouteMode::Success(source) | FakeSourceRouteMode::Degraded(source) => {
                Ok(source.clone())
            }
            FakeSourceRouteMode::Failure(error) => Err(error.clone()),
        }
    }

    async fn capabilities(&self) -> Result<SourceResolverCapability> {
        if let Some(capability) = &self.capability_override {
            return Ok(capability.clone());
        }
        let health = match &self.mode {
            FakeSourceRouteMode::Success(_) => HealthStatus::Healthy,
            FakeSourceRouteMode::Degraded(_) => HealthStatus::Degraded,
            FakeSourceRouteMode::Failure(_) => HealthStatus::Unavailable,
        };
        Ok(SourceResolverCapability::from(CapabilityBase {
            name: "fake-source-resolver".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-route".to_string(),
            health,
            features: vec!["fake".to_string()],
            limits: MetadataMap::new(),
        }))
    }
}

#[derive(Debug, Clone)]
pub struct FakeSourceRouter {
    mode: FakeSourceRouteMode<RoutePlan>,
    calls: Arc<Mutex<Vec<(ResolvedSource, SourceRequest)>>>,
    capability_override: Option<SourceRouterCapability>,
}

impl FakeSourceRouter {
    pub fn new(mode: FakeSourceRouteMode<RoutePlan>) -> Self {
        Self {
            mode,
            calls: Arc::new(Mutex::new(Vec::new())),
            capability_override: None,
        }
    }

    pub fn with_capability_override(mut self, capability: SourceRouterCapability) -> Self {
        self.capability_override = Some(capability);
        self
    }

    pub async fn calls(&self) -> Vec<(ResolvedSource, SourceRequest)> {
        self.calls
            .lock()
            .expect("fake source router call log mutex poisoned")
            .clone()
    }

    fn record(&self, source: &ResolvedSource, request: &SourceRequest) {
        self.calls
            .lock()
            .expect("fake source router call log mutex poisoned")
            .push((source.clone(), request.clone()));
    }
}

#[async_trait]
impl SourceRouter for FakeSourceRouter {
    async fn route(&self, source: ResolvedSource, request: &SourceRequest) -> Result<RoutePlan> {
        self.record(&source, request);
        match &self.mode {
            FakeSourceRouteMode::Success(plan) | FakeSourceRouteMode::Degraded(plan) => {
                Ok(plan.clone())
            }
            FakeSourceRouteMode::Failure(error) => Err(error.clone()),
        }
    }

    async fn validate_options(&self, plan: &RoutePlan) -> Result<ValidatedOptions> {
        if let FakeSourceRouteMode::Failure(error) = &self.mode {
            return Err(error.clone());
        }
        Ok(ValidatedOptions {
            values: plan.validated_options.values.clone(),
            warnings: Vec::new(),
        })
    }

    async fn capabilities(&self) -> Result<SourceRouterCapability> {
        if let Some(capability) = &self.capability_override {
            return Ok(capability.clone());
        }
        let health = match &self.mode {
            FakeSourceRouteMode::Success(_) => HealthStatus::Healthy,
            FakeSourceRouteMode::Degraded(_) => HealthStatus::Degraded,
            FakeSourceRouteMode::Failure(_) => HealthStatus::Unavailable,
        };
        Ok(SourceRouterCapability::from(CapabilityBase {
            name: "fake-source-router".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-route".to_string(),
            health,
            features: vec!["fake".to_string()],
            limits: MetadataMap::new(),
        }))
    }
}

#[cfg(test)]
#[path = "boundary_tests.rs"]
mod tests;
