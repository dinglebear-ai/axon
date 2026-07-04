use axon_api::source::{HealthStatus, ReservationState};

use crate::fake::FakeEmbeddingProvider;
use crate::provider::EmbeddingProvider;

#[tokio::test]
async fn fake_provider_with_zero_dimensions_reports_unavailable_capability() {
    let capability = FakeEmbeddingProvider::new("fake-embedding", 0)
        .capabilities()
        .await
        .unwrap();

    assert_eq!(capability.health, HealthStatus::Unavailable);
    assert_eq!(
        capability.last_error.as_ref().unwrap().code.to_string(),
        "provider.invalid_dimensions"
    );
    assert_eq!(capability.reservation_state.available_units, 0);
    assert_eq!(
        capability.reservation_state.states,
        vec![ReservationState::Failed]
    );
}
