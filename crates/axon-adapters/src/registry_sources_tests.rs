use axon_api::source::*;

use crate::SourceAdapter;
use crate::registry_sources::RegistrySourceAdapter;
use crate::registry_sources_test_support::*;

#[tokio::test]
async fn registry_adapter_declares_package_and_version_scopes() {
    let adapter = RegistrySourceAdapter::new();

    let capability = adapter.capabilities().await.unwrap();
    assert_eq!(capability.0.name, "registry");
    assert_eq!(
        capability.0.limits.0.get("source_kind"),
        Some(&serde_json::json!(SourceKind::Registry))
    );
    assert_eq!(
        capability.0.limits.0.get("default_scope"),
        Some(&serde_json::json!(SourceScope::Package))
    );
    assert!(capability.0.features.contains(&"scope:package".to_string()));
    assert!(capability.0.features.contains(&"scope:version".to_string()));
}

#[tokio::test]
async fn registry_adapter_rejects_unknown_options() {
    let adapter = RegistrySourceAdapter::new();
    let dump_path = write_dump(valid_dump_json());
    let mut plan = source_plan(dump_path, SourceScope::Package);
    plan.route
        .validated_options
        .values
        .insert("surprise".to_string(), "nope".into());

    let err = adapter
        .discover(&plan)
        .await
        .expect_err("unknown registry options should fail validation");

    assert_eq!(err.code.0, "adapter.registry.option.invalid");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
}

#[tokio::test]
async fn registry_adapter_rejects_missing_dump_path_option() {
    let adapter = RegistrySourceAdapter::new();
    let mut plan = source_plan(write_dump(valid_dump_json()), SourceScope::Package);
    plan.route.validated_options.values.clear();

    let err = adapter
        .discover(&plan)
        .await
        .expect_err("missing registry_dump_path should fail validation");

    assert_eq!(err.code.0, "adapter.registry.option.invalid");
}

#[tokio::test]
async fn registry_discovery_defaults_to_latest_version_only() {
    let adapter = RegistrySourceAdapter::new();
    let dump_path = write_dump(valid_dump_json());
    let plan = source_plan(dump_path, SourceScope::Package);

    let manifest = adapter.discover(&plan).await.unwrap();

    assert_eq!(manifest.items.len(), 1);
    let item = &manifest.items[0];
    assert_eq!(item.version.as_deref(), Some("4.17.21"));
    assert_eq!(item.item_kind, ItemKind::PackageVersion);
    assert_eq!(item.content_kind, Some(ContentKind::Markdown));
    assert_eq!(
        item.source_item_key,
        SourceItemKey::from("versions/4.17.21")
    );
    assert_eq!(item.canonical_uri, "pkg://npm/lodash/versions/4.17.21");
}

#[tokio::test]
async fn registry_discovery_includes_all_versions_when_requested() {
    let adapter = RegistrySourceAdapter::new();
    let dump_path = write_dump(valid_dump_json());
    let mut plan = source_plan(dump_path.clone(), SourceScope::Package);
    plan.route.validated_options.values = registry_options_all_versions(&dump_path);

    let manifest = adapter.discover(&plan).await.unwrap();

    let versions = manifest
        .items
        .iter()
        .map(|item| item.version.as_deref().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(versions, vec!["4.17.20", "4.17.21"]);
}

#[tokio::test]
async fn registry_adapter_acquires_and_normalizes_source_documents() {
    let adapter = RegistrySourceAdapter::new();
    let dump_path = write_dump(valid_dump_json());
    let plan = source_plan(dump_path, SourceScope::Package);

    let manifest = adapter.discover(&plan).await.unwrap();
    let diff = manifest_diff(&plan, manifest.items.clone());

    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert_eq!(acquisition.fetched_items.len(), 1);
    let ContentRef::InlineText { text } = &acquisition.fetched_items[0].content_ref else {
        panic!("expected inline text content for registry acquisition");
    };
    assert!(text.contains("lodash@4.17.21"));
    assert!(text.contains("A modern JavaScript utility library."));
    assert!(text.contains("**License:** MIT"));
    assert!(text.contains("## README"));

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    assert_eq!(normalized.header.phase, PipelinePhase::Normalizing);
    let documents = normalized.data;
    assert_eq!(documents.len(), 1);
    let document = &documents[0];
    assert_eq!(document.source_id, SourceId::from("src_registry_test"));
    assert_eq!(document.title.as_deref(), Some("lodash@4.17.21"));
    assert_eq!(document.content_kind, ContentKind::Markdown);
    assert_eq!(
        document
            .metadata
            .get("pkg_registry")
            .and_then(|v| v.as_str()),
        Some("npm")
    );
    assert_eq!(
        document.metadata.get("pkg_name").and_then(|v| v.as_str()),
        Some("lodash")
    );
    assert_eq!(
        document
            .metadata
            .get("pkg_version")
            .and_then(|v| v.as_str()),
        Some("4.17.21")
    );
    assert_eq!(
        document
            .metadata
            .get("source_kind")
            .and_then(|v| v.as_str()),
        Some("registry")
    );
}

/// The registry adapter is generic over `RegistryDump.registry` — this
/// resolves a non-npm ecosystem (huggingface) end to end (discover →
/// acquire → normalize) to prove the 13-ecosystem expansion actually
/// resolves, not just that npm still works.
#[tokio::test]
async fn registry_adapter_resolves_a_newly_added_ecosystem_end_to_end() {
    let adapter = RegistrySourceAdapter::new();
    let dump_path = write_dump(huggingface_dump_json());
    let plan = source_plan_for(
        "pkg://huggingface/bert-base-uncased",
        dump_path,
        SourceScope::Package,
    );

    let manifest = adapter.discover(&plan).await.unwrap();
    assert_eq!(manifest.items.len(), 1);
    assert_eq!(
        manifest.items[0].canonical_uri,
        "pkg://huggingface/bert-base-uncased/versions/main"
    );

    let diff = manifest_diff(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert_eq!(acquisition.fetched_items.len(), 1);
    let ContentRef::InlineText { text } = &acquisition.fetched_items[0].content_ref else {
        panic!("expected inline text content for registry acquisition");
    };
    assert!(text.contains("bert-base-uncased@main"));
    assert!(text.contains("BERT base model (uncased)"));

    let normalized = adapter.normalize(&plan, acquisition).await.unwrap();
    let documents = normalized.data;
    assert_eq!(documents.len(), 1);
    let document = &documents[0];
    assert_eq!(
        document
            .metadata
            .get("pkg_registry")
            .and_then(|v| v.as_str()),
        Some("huggingface")
    );
    assert_eq!(
        document.metadata.get("pkg_name").and_then(|v| v.as_str()),
        Some("bert-base-uncased")
    );
    assert_eq!(
        document
            .metadata
            .get("pkg_version")
            .and_then(|v| v.as_str()),
        Some("main")
    );
}

#[tokio::test]
async fn registry_adapter_reports_error_for_missing_dump_file() {
    let adapter = RegistrySourceAdapter::new();
    let plan = source_plan(
        std::path::PathBuf::from("/nonexistent/axon-registry-dump.json"),
        SourceScope::Package,
    );

    let err = adapter
        .discover(&plan)
        .await
        .expect_err("missing dump file should fail discovery");

    assert_eq!(err.code.0, "adapter.registry.dump_unreadable");
}

#[tokio::test]
async fn registry_adapter_reports_error_for_malformed_dump() {
    let adapter = RegistrySourceAdapter::new();
    let dump_path = write_dump("not json");
    let plan = source_plan(dump_path, SourceScope::Package);

    let err = adapter
        .discover(&plan)
        .await
        .expect_err("malformed dump JSON should fail discovery");

    assert_eq!(err.code.0, "adapter.registry.dump_malformed");
}

#[tokio::test]
async fn registry_adapter_reports_error_for_empty_versions_dump() {
    let adapter = RegistrySourceAdapter::new();
    let dump_path = write_dump(r#"{"registry": "npm", "package": "lodash", "versions": []}"#);
    let plan = source_plan(dump_path, SourceScope::Package);

    let err = adapter
        .discover(&plan)
        .await
        .expect_err("empty versions dump should fail discovery");

    assert_eq!(err.code.0, "adapter.registry.dump_invalid");
}

#[tokio::test]
async fn registry_adapter_rejects_mismatched_route_adapter() {
    let adapter = RegistrySourceAdapter::new();
    let dump_path = write_dump(valid_dump_json());
    let mut plan = source_plan(dump_path, SourceScope::Package);
    plan.route.adapter.name = "web".to_string();

    let err = adapter
        .discover(&plan)
        .await
        .expect_err("mismatched adapter route should be rejected");

    assert_eq!(err.code.0, "adapter.registry.mismatch");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
}
