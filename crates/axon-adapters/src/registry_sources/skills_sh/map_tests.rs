use super::*;

#[test]
fn canonical_github_pointer_uses_install_url_without_inventing_source_path() {
    let skill = SkillsShSkill {
        id: "vercel-labs/skills/find-skills".to_string(),
        slug: "find-skills".to_string(),
        name: "Find Skills".to_string(),
        source: "vercel-labs/skills".to_string(),
        installs: 42,
        source_type: "github".to_string(),
        install_url: Some("https://github.com/vercel-labs/skills.git".to_string()),
        url: Some("https://skills.sh/vercel-labs/skills/find-skills".to_string()),
        is_duplicate: Some(false),
        audits: Vec::new(),
        audit_status: None,
        audit_warnings: Vec::new(),
    };
    let (uri, repo, warnings) = canonical_source_pointer(&skill);
    assert_eq!(uri, "https://github.com/vercel-labs/skills");
    assert_eq!(repo.as_deref(), Some("vercel-labs/skills"));
    assert!(warnings.is_empty());
}

#[test]
fn github_pointer_rejects_unrelated_install_host_and_falls_back_to_skills_sh() {
    let skill = SkillsShSkill {
        id: "vercel-labs/skills/find-skills".to_string(),
        slug: "find-skills".to_string(),
        name: "Find Skills".to_string(),
        source: "vercel-labs/skills".to_string(),
        installs: 42,
        source_type: "github".to_string(),
        install_url: Some("https://example.invalid/vercel-labs/skills".to_string()),
        url: Some("https://skills.sh/vercel-labs/skills/find-skills".to_string()),
        is_duplicate: None,
        audits: Vec::new(),
        audit_status: None,
        audit_warnings: Vec::new(),
    };

    let (uri, repo, warnings) = canonical_source_pointer(&skill);

    assert_eq!(uri, "https://skills.sh/vercel-labs/skills/find-skills");
    assert!(repo.is_none());
    assert_eq!(warnings.len(), 1);
}

#[test]
fn catalog_observation_time_never_falls_back_to_wall_clock() {
    let document = SourceDocument {
        document_id: DocumentId::from("doc_test"),
        source_id: SourceId::from("src_test"),
        source_item_key: SourceItemKey::from("item_test"),
        canonical_uri: "catalog://skills.sh/item".to_string(),
        content_kind: ContentKind::Structured,
        content: ContentRef::InlineText {
            text: "{}".to_string(),
        },
        metadata: MetadataMap::new(),
        title: None,
        language: None,
        path: None,
        mime_type: Some("application/json".to_string()),
        structured_payload: Some(serde_json::json!({})),
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    };

    let error = catalog_observed_at(&document).expect_err("timestamp must be evidence-backed");
    assert_eq!(error.code.0, "adapter.skills_sh.observed_at_missing");
}
