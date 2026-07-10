use axon_api::source::{
    ChunkHint, ChunkProfile, ContentKind, ContentRef, DocumentId, MetadataMap, SourceDocument,
    SourceId, SourceItemKey,
};
use serde_json::json;

use crate::chunk_router::decision_for_profile;
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
fn chunk_profile_completeness_covers_required_profiles() {
    let cases = [
        (
            source_doc(ContentKind::Code, "pub fn run() {}\n").with_path("src/lib.rs"),
            ChunkingProfile::CodeSymbol,
        ),
        (
            source_doc(ContentKind::PlainText, "FROM alpine\n").with_path("Dockerfile"),
            ChunkingProfile::CodeManifest,
        ),
        (
            source_doc(ContentKind::Markdown, "# Heading\n"),
            ChunkingProfile::MarkdownSections,
        ),
        (
            source_doc(ContentKind::Html, "<article>Body</article>"),
            ChunkingProfile::HtmlArticle,
        ),
        (
            source_doc(ContentKind::PlainText, "plain text"),
            ChunkingProfile::PlainTextWindows,
        ),
        (
            source_doc(ContentKind::Transcript, "user: hi"),
            ChunkingProfile::TranscriptSegments,
        ),
        (
            source_doc(ContentKind::PlainText, "PORT=3000").with_path(".env.example"),
            ChunkingProfile::StructuredRecords,
        ),
        (
            source_doc(ContentKind::Yaml, "openapi: 3.1.0").with_path("openapi.yaml"),
            ChunkingProfile::ApiSchema,
        ),
        (
            source_doc(ContentKind::PlainText, r#"{"tool":"shell"}"#)
                .with_path("tool-output.jsonl"),
            ChunkingProfile::ToolOutput,
        ),
        (
            source_doc(ContentKind::PlainText, r#"{"role":"user"}"#).with_path("session.jsonl"),
            ChunkingProfile::SessionTurns,
        ),
        (
            source_doc(ContentKind::BinaryMetadata, "meta"),
            ChunkingProfile::AtomicMetadata,
        ),
    ];

    for (doc, expected) in cases {
        assert_eq!(ChunkRouter::default().route(&doc).unwrap(), expected);
    }
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

// -- ChunkRouter full decision: method/parser_family/fallback_chain/limits,
// and adapter/scope/size-aware routing (S2-19). --------------------------

#[test]
fn large_document_falls_back_to_second_chain_step_for_wired_profiles() {
    let large = 200_001;
    let small = 199_999;

    for profile in [
        ChunkingProfile::CodeSymbol,
        ChunkingProfile::MarkdownSections,
        ChunkingProfile::HtmlArticle,
    ] {
        let small_decision = decision_for_profile(profile, small, None, None);
        assert_eq!(
            small_decision.method, small_decision.fallback_chain[0],
            "{profile:?} under the size threshold should use its primary method"
        );

        let large_decision = decision_for_profile(profile, large, None, None);
        assert_eq!(
            large_decision.method, large_decision.fallback_chain[1],
            "{profile:?} over the size threshold should report its wired fallback method"
        );
    }
}

#[test]
fn large_document_does_not_override_method_for_unwired_profiles() {
    // These profiles' fallback_chain[1] is never actually dispatched to by
    // `preparer::build_chunks` for a size trigger (structured profiles run
    // their own parse-failure fallback; the rest are already line/turn-based
    // regardless of size), so the router must not claim it ran.
    for profile in [
        ChunkingProfile::CodeManifest,
        ChunkingProfile::PlainTextWindows,
        ChunkingProfile::TranscriptSegments,
        ChunkingProfile::StructuredRecords,
        ChunkingProfile::ApiSchema,
        ChunkingProfile::ToolOutput,
        ChunkingProfile::SessionTurns,
        ChunkingProfile::AtomicMetadata,
    ] {
        let decision = decision_for_profile(profile, 500_000, None, None);
        assert_eq!(
            decision.method, decision.fallback_chain[0],
            "{profile:?} has no wired size fallback; method must stay the primary"
        );
    }
}

#[test]
fn fragment_prone_adapter_forces_fallback_method_even_for_small_documents() {
    let decision = decision_for_profile(
        ChunkingProfile::CodeSymbol,
        200, // tiny -- well under the size threshold
        Some("web_scrape"),
        None,
    );
    assert_eq!(decision.method, decision.fallback_chain[1]);

    let decision = decision_for_profile(ChunkingProfile::MarkdownSections, 200, Some("chat"), None);
    assert_eq!(decision.method, decision.fallback_chain[1]);
}

#[test]
fn trusted_adapter_keeps_primary_method_for_small_documents() {
    let decision = decision_for_profile(ChunkingProfile::CodeSymbol, 200, Some("filesystem"), None);
    assert_eq!(decision.method, decision.fallback_chain[0]);
}

#[test]
fn partial_scope_halves_the_chunk_token_budget() {
    let full = decision_for_profile(ChunkingProfile::MarkdownSections, 500, None, None);
    let partial = decision_for_profile(ChunkingProfile::MarkdownSections, 500, None, Some("diff"));

    assert_eq!(
        partial.limits.max_chunk_tokens,
        full.limits.max_chunk_tokens / 2
    );
    assert_eq!(
        partial.limits.overlap_tokens,
        full.limits.overlap_tokens / 2
    );
    // Unaffected fields stay identical.
    assert_eq!(partial.method, full.method);
    assert_eq!(partial.parser_family, full.parser_family);
}

#[test]
fn partial_scope_never_drops_the_token_budget_below_the_floor() {
    // AtomicMetadata's overlap is already 0 and its ceiling (1600) is well
    // above the floor, but this pins the floor behavior generically in case
    // any profile's ceiling is ever tuned down near it.
    let decision =
        decision_for_profile(ChunkingProfile::AtomicMetadata, 500, None, Some("fragment"));
    assert!(decision.limits.max_chunk_tokens >= 200);
}

#[test]
fn route_decision_reads_adapter_and_scope_from_document_metadata() {
    let mut doc = source_doc(ContentKind::Code, "xxxxxxxxxx").with_path("snippet.rs");
    doc.metadata = metadata([
        ("source_adapter", json!("web_scrape")),
        ("source_scope", json!("diff")),
    ]);

    let decision = ChunkRouter::default().route_decision(&doc).unwrap();
    assert_eq!(decision.profile, ChunkingProfile::CodeSymbol);
    // Fragment-prone adapter forces the fallback method even though the
    // document is tiny.
    assert_eq!(decision.method, decision.fallback_chain[1]);
    // Partial scope halves the limits relative to the untagged decision.
    let baseline = decision_for_profile(ChunkingProfile::CodeSymbol, 10, None, None);
    assert_eq!(
        decision.limits.max_chunk_tokens,
        baseline.limits.max_chunk_tokens / 2
    );
}
