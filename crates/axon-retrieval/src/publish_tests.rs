use axon_api::source::{PublishGenerationRequest, SourceGenerationId, SourceId};

use super::{GenerationPublisher, InMemoryGenerationPublisher};

fn request(source: &str, generation: &str) -> PublishGenerationRequest {
    PublishGenerationRequest {
        source_id: SourceId::from(source),
        generation: SourceGenerationId::from(generation),
        expected_previous_generation: None,
    }
}

#[tokio::test]
async fn validate_publish_is_ready_with_no_prior_generation() {
    let publisher = InMemoryGenerationPublisher::new();

    let plan = publisher
        .validate_publish(request("src_web", "gen_1"))
        .await
        .unwrap();

    assert!(plan.ready);
    assert!(plan.previous_generation.is_none());
    assert!(plan.warnings.is_empty());
}

#[tokio::test]
async fn publish_generation_commits_and_is_visible_to_later_validate_calls() {
    let publisher = InMemoryGenerationPublisher::new();

    let result = publisher
        .publish_generation(request("src_web", "gen_1"))
        .await
        .unwrap();
    assert_eq!(result.source_id, SourceId::from("src_web"));
    assert_eq!(result.generation, SourceGenerationId::from("gen_1"));

    let mut next = request("src_web", "gen_2");
    next.expected_previous_generation = Some(SourceGenerationId::from("gen_1"));
    let plan = publisher.validate_publish(next).await.unwrap();

    assert!(plan.ready);
    assert_eq!(
        plan.previous_generation,
        Some(SourceGenerationId::from("gen_1"))
    );
}

#[tokio::test]
async fn publish_generation_rejects_stale_expected_previous_generation() {
    let publisher = InMemoryGenerationPublisher::new();
    publisher
        .publish_generation(request("src_web", "gen_1"))
        .await
        .unwrap();

    let mut stale = request("src_web", "gen_3");
    stale.expected_previous_generation = Some(SourceGenerationId::from("gen_0"));

    let plan = publisher.validate_publish(stale.clone()).await.unwrap();
    assert!(!plan.ready);
    assert_eq!(plan.warnings.len(), 1);

    let result = publisher.publish_generation(stale).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn publish_generation_rejects_expected_previous_generation_when_none_committed() {
    let publisher = InMemoryGenerationPublisher::new();
    let mut request = request("src_web", "gen_1");
    request.expected_previous_generation = Some(SourceGenerationId::from("gen_0"));

    let result = publisher.publish_generation(request).await;

    assert!(result.is_err());
}
