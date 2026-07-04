use axon_api::source::{JobPriority, ProviderId};

use crate::reservation::ProviderReservations;

#[tokio::test]
async fn compatibility_provider_reservations_keep_legacy_per_provider_api() {
    let reservations = ProviderReservations::new(2, 1);

    let held = reservations
        .reserve(
            ProviderId::new("fake-embedding"),
            JobPriority::Interactive,
            1,
        )
        .await
        .unwrap();

    assert_eq!(held.provider_id(), &ProviderId::new("fake-embedding"));
    assert_eq!(reservations.snapshot().await.active, 1);
}

#[tokio::test]
async fn compatibility_provider_reservations_share_capacity_across_provider_ids() {
    let reservations = ProviderReservations::new(2, 0);

    let _first = reservations
        .reserve(ProviderId::new("fake-a"), JobPriority::Interactive, 1)
        .await
        .unwrap();
    let _second = reservations
        .reserve(ProviderId::new("fake-b"), JobPriority::Interactive, 1)
        .await
        .unwrap();

    let denied = reservations
        .reserve(ProviderId::new("fake-c"), JobPriority::Interactive, 1)
        .await
        .unwrap_err();

    assert_eq!(denied.code.to_string(), "provider.capacity_exhausted");
    assert_eq!(denied.provider_id, Some("fake-c".to_string()));
    assert_eq!(reservations.snapshot().await.available_units, 0);
}
