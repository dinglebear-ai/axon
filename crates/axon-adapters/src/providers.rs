//! Real (non-fake) [`crate::boundary::FetchProvider`] /
//! [`crate::boundary::RenderProvider`] / [`crate::boundary::SearchProvider`]
//! implementations.
//!
//! These are also the acquisition primitives issue #298's real
//! `WebSourceAdapter` slice calls. Wiring status per provider:
//! [`http_fetch::HttpFetchProvider`] and [`chrome_render::ChromeRenderProvider`]
//! are wired into `axon-services`' `TargetLocalSourceRuntime`
//! (`crates/axon-services/src/context/target_runtime.rs`) and, through it,
//! `WebSourceAdapter`. [`searxng_search::SearxngSearchProvider`] and
//! [`tavily_search::TavilySearchProvider`] are wired into `axon-services`'
//! `search`/`research` commands (`crates/axon-services/src/search/provider.rs`,
//! issue #298 WS-D) but not into `WebSourceAdapter` — search/research are not
//! source-acquisition operations.

pub mod chrome_render;
pub mod http_fetch;
pub mod searxng_search;
pub mod tavily_search;

use axon_api::source::{HealthStatus, ReservationState, ReservationStateSnapshot};

/// Shared reservation-state snapshot builder for [`http_fetch::HttpFetchProvider`],
/// [`chrome_render::ChromeRenderProvider`], [`searxng_search::SearxngSearchProvider`],
/// and [`tavily_search::TavilySearchProvider`].
///
/// None of these providers does internal batching/leasing (each call is one
/// request), so `available_units` is a simple 0/1 flag rather than a
/// concurrency count: `0` while `health` is `Cooling`/`Unavailable`, `1`
/// otherwise. Deliberately NOT derived from `ProviderReservationManager::snapshot()`
/// — that reflects `reserve()`/`ProviderReservation` activity (which these
/// providers never call), so it would always report full capacity regardless
/// of health. Mirrors `axon-embedding`'s `embedding_reservation_state` helper.
pub(crate) fn single_slot_reservation_state(health: HealthStatus) -> ReservationStateSnapshot {
    let available = u32::from(!matches!(
        health,
        HealthStatus::Cooling | HealthStatus::Unavailable
    ));
    ReservationStateSnapshot {
        queued: 0,
        active: 0,
        available_units: available,
        oldest_queued_ms: None,
        priority_breakdown: Default::default(),
        states: if available == 0 {
            vec![ReservationState::Failed]
        } else {
            vec![ReservationState::Granted]
        },
    }
}
