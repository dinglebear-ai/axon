use axon_api::source::{
    ChunkHint, ChunkProfile, ContentKind, ContentRef, DocumentId, MetadataMap, SourceDocument,
    SourceId, SourceItemKey,
};
use serde_json::json;

use crate::{ChunkRouter, ChunkingProfile, chunk_router::public_profiles};

#[test]
fn router_honors_typed_pr8_profiles_from_chunk_hints() {
    for (profile, expected) in public_profiles() {
        let mut doc = source_doc(ContentKind::PlainText, "body");
        doc.chunk_hints = vec![ChunkHint {
            profile,
            reason: "test override".to_string(),
            options: MetadataMap::new(),
        }];

        assert_eq!(
            ChunkRouter::default().route(&doc).unwrap(),
            expected,
            "typed profile override should route"
        );
    }
}

#[test]
fn typed_chunk_hint_wins_over_metadata_profile_escape_hatch() {
    let mut doc = source_doc(ContentKind::PlainText, "body");
    doc.chunk_hints = vec![ChunkHint {
        profile: ChunkProfile::PlainTextWindows,
        reason: "test override".to_string(),
        options: metadata([("axon_document_profile", json!("code_symbol"))]),
    }];

    assert_eq!(
        ChunkRouter::default().route(&doc).unwrap(),
        ChunkingProfile::PlainTextWindows
    );
}

#[test]
fn router_honors_metadata_profile_when_no_typed_hint_exists() {
    let mut doc = source_doc(ContentKind::PlainText, "body");
    doc.metadata = metadata([("axon_document_profile", json!("code_symbol"))]);

    assert_eq!(
        ChunkRouter::default().route(&doc).unwrap(),
        ChunkingProfile::CodeSymbol
    );
}

#[test]
fn router_selects_profile_from_document_shape_when_no_override_exists() {
    let cases = [
        (
            source_doc(ContentKind::Code, "fn main() {}").with_path("src/main.rs"),
            ChunkingProfile::CodeSymbol,
        ),
        (
            source_doc(ContentKind::Toml, "[package]\nname = \"axon\"").with_path("Cargo.toml"),
            ChunkingProfile::CodeManifest,
        ),
        (
            source_doc(ContentKind::Markdown, "# Title\nBody"),
            ChunkingProfile::MarkdownSections,
        ),
        (
            source_doc(
                ContentKind::Html,
                "<article><h1>Title</h1><p>Body</p></article>",
            ),
            ChunkingProfile::HtmlArticle,
        ),
        (
            source_doc(ContentKind::Transcript, "00:00 Speaker: hello"),
            ChunkingProfile::TranscriptSegments,
        ),
        (
            source_doc(ContentKind::Json, "{\"openapi\":\"3.1.0\",\"paths\":{}}")
                .with_path("openapi.json"),
            ChunkingProfile::ApiSchema,
        ),
        (
            source_doc(ContentKind::BinaryMetadata, "metadata only"),
            ChunkingProfile::AtomicMetadata,
        ),
    ];

    for (doc, expected) in cases {
        assert_eq!(ChunkRouter::default().route(&doc).unwrap(), expected);
    }
}

#[test]
fn router_selects_phase_7_parser_profiles_by_path() {
    assert_eq!(route_for_path("Dockerfile"), ChunkingProfile::CodeManifest);
    assert_eq!(
        route_for_path("docker-compose.yml"),
        ChunkingProfile::CodeManifest
    );
    assert_eq!(
        route_for_path(".env.example"),
        ChunkingProfile::StructuredRecords
    );
    assert_eq!(
        route_for_path("tool-output.jsonl"),
        ChunkingProfile::ToolOutput
    );
}

#[test]
fn router_ignores_generic_profile_metadata() {
    let mut doc = source_doc(ContentKind::PlainText, "release profile text");
    doc.metadata = metadata([("profile", json!("production"))]);

    assert_eq!(
        ChunkRouter::default().route(&doc).unwrap(),
        ChunkingProfile::PlainTextWindows
    );
}

#[test]
fn router_recognizes_common_manifest_and_config_files() {
    for path in [
        "requirements.txt",
        "docker-compose.yaml",
        "Dockerfile",
        ".env.example",
        "main.tf",
        "openapi.yaml",
        "schema.graphql",
        "service.proto",
    ] {
        let doc = source_doc(ContentKind::PlainText, "name=value").with_path(path);
        let expected = if matches!(path, "openapi.yaml" | "schema.graphql" | "service.proto") {
            ChunkingProfile::ApiSchema
        } else if matches!(path, ".env.example") {
            ChunkingProfile::StructuredRecords
        } else {
            ChunkingProfile::CodeManifest
        };

        assert_eq!(
            ChunkRouter::default().route(&doc).unwrap(),
            expected,
            "{path} should route to the expected profile"
        );
    }
}

trait SourceDocTestExt {
    fn with_path(self, path: &str) -> Self;
}

impl SourceDocTestExt for SourceDocument {
    fn with_path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }
}

fn source_doc(content_kind: ContentKind, text: &str) -> SourceDocument {
    SourceDocument {
        document_id: DocumentId::from("doc-test"),
        source_id: SourceId::from("src-test"),
        source_item_key: SourceItemKey::from("item-test"),
        canonical_uri: "file:///test".to_string(),
        content_kind,
        content: ContentRef::InlineText {
            text: text.to_string(),
        },
        metadata: MetadataMap::new(),
        title: None,
        language: None,
        path: None,
        mime_type: None,
        structured_payload: None,
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    }
}

fn metadata(entries: impl IntoIterator<Item = (&'static str, serde_json::Value)>) -> MetadataMap {
    let mut map = MetadataMap::new();
    for (key, value) in entries {
        map.insert(key.to_string(), value);
    }
    map
}

fn route_for_path(path: &str) -> ChunkingProfile {
    ChunkRouter::default()
        .route(&source_doc(ContentKind::PlainText, "body").with_path(path))
        .unwrap()
}
