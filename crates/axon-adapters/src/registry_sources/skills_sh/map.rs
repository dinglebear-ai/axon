//! Pure skills.sh dump -> SourceManifest/SourceDocument/ArtifactCandidate mapping.

use std::collections::BTreeMap;

use axon_api::source::*;
use sha2::{Digest, Sha256};
use url::Url;

use crate::adapter::Result;
use crate::artifact_candidates::{artifact_candidate_dedupe, artifact_candidate_id};
use crate::manifest::item_identity;

use super::{SkillsShDump, SkillsShSkill};

pub(crate) fn discover(plan: &SourcePlan, dump: &SkillsShDump) -> Result<SourceManifest> {
    let mut items = Vec::with_capacity(dump.skills.len());
    for skill in &dump.skills {
        let identity = item_identity(
            SourceKind::Registry,
            &plan.route.source.canonical_uri,
            &skill.id,
        )?;
        items.push(ManifestItem {
            source_id: plan.route.source.source_id.clone(),
            source_item_key: identity.source_item_key,
            canonical_uri: identity.canonical_uri,
            item_kind: ItemKind::Artifact,
            content_kind: Some(ContentKind::Structured),
            display_path: Some(skill.id.clone()),
            parent_key: None,
            size_bytes: None,
            content_hash: Some(listing_digest(skill)?),
            mtime: None,
            version: normalized_sha256(skill.hash.as_deref()),
            fetch_plan: None,
            metadata: MetadataMap::new(),
            graph_hints: Vec::new(),
        });
    }
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));
    Ok(SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: SourceGenerationId::from("gen_skills_sh_discovery"),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items,
        created_at: dump.observed_at.clone(),
        metadata: MetadataMap::new(),
    })
}

pub(crate) fn acquire(
    plan: &SourcePlan,
    diff: &SourceManifestDiff,
    dump: &SkillsShDump,
) -> Result<SourceAcquisition> {
    let by_key = dump
        .skills
        .iter()
        .map(|skill| Ok((source_item_key(plan, skill)?, skill)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let manifest_items = diff
        .added
        .iter()
        .chain(diff.modified.iter())
        .cloned()
        .collect::<Vec<_>>();
    let mut fetched_items = Vec::with_capacity(manifest_items.len());
    for item in &manifest_items {
        let skill = by_key.get(&item.source_item_key).ok_or_else(|| {
            ApiError::new(
                "adapter.skills_sh.item_missing",
                ErrorStage::Fetching,
                "skills.sh dump is missing a changed manifest item",
            )
            .with_context("source_item_key", item.source_item_key.0.clone())
        })?;
        let payload = safe_listing_payload(skill)?;
        let text = serde_json::to_string_pretty(&payload).map_err(|error| {
            ApiError::new(
                "adapter.skills_sh.payload_serialize_failed",
                ErrorStage::Fetching,
                error.to_string(),
            )
        })?;
        fetched_items.push(AcquiredSourceItem {
            manifest_item: item.clone(),
            fetch_status: LifecycleStatus::Completed,
            content_ref: ContentRef::InlineText { text },
            raw_artifact_id: None,
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            fetched_at: dump.observed_at.clone(),
            metadata: MetadataMap::new(),
        });
    }
    let manifest = SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: diff.next_generation.clone(),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items: manifest_items,
        created_at: dump.observed_at.clone(),
        metadata: MetadataMap::new(),
    };
    Ok(SourceAcquisition {
        header: super::super::stage_header(
            plan.job_id,
            "skills_sh_fetch",
            PipelinePhase::Fetching,
            fetched_items.len(),
        ),
        source_id: manifest.source_id.clone(),
        generation: manifest.generation.clone(),
        adapter: manifest.adapter.clone(),
        scope: manifest.scope,
        manifest,
        fetched_items,
        artifacts: Vec::new(),
    })
}

pub(crate) fn normalize(
    plan: &SourcePlan,
    acquisition: &SourceAcquisition,
    dump: &SkillsShDump,
) -> Result<StageExecutionResult<Vec<SourceDocument>>> {
    let by_key = dump
        .skills
        .iter()
        .map(|skill| Ok((source_item_key(plan, skill)?, skill)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let documents = acquisition
        .fetched_items
        .iter()
        .map(|item| {
            let skill = by_key
                .get(&item.manifest_item.source_item_key)
                .ok_or_else(|| {
                    ApiError::new(
                        "adapter.skills_sh.item_missing",
                        ErrorStage::Normalizing,
                        "skills.sh dump is missing a fetched manifest item",
                    )
                })?;
            let payload = safe_listing_payload(skill)?;
            let mut metadata = MetadataMap::new();
            metadata.insert(
                "catalog_provider".to_string(),
                serde_json::json!("skills.sh"),
            );
            metadata.insert("skills_sh_id".to_string(), serde_json::json!(skill.id));
            metadata.insert(
                "source_type".to_string(),
                serde_json::json!(skill.source_type),
            );
            metadata.insert("installs".to_string(), serde_json::json!(skill.installs));
            metadata.insert(
                "catalog_observed_at".to_string(),
                serde_json::json!(dump.observed_at.0),
            );
            if let Some(duplicate) = skill.is_duplicate {
                metadata.insert("is_duplicate".to_string(), serde_json::json!(duplicate));
            }
            Ok(SourceDocument {
                document_id: DocumentId::from(format!("doc_skills_sh_{}", digest_id(&skill.id))),
                source_id: acquisition.source_id.clone(),
                source_item_key: item.manifest_item.source_item_key.clone(),
                canonical_uri: item.manifest_item.canonical_uri.clone(),
                content_kind: ContentKind::Structured,
                content: item.content_ref.clone(),
                metadata,
                title: Some(skill.name.clone()),
                language: None,
                path: None,
                mime_type: Some("application/json".to_string()),
                structured_payload: Some(payload),
                artifact_id: None,
                chunk_hints: plan.route.chunking_hints.clone(),
                parser_hints: plan.route.parser_hints.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(StageExecutionResult {
        header: super::super::stage_header(
            plan.job_id,
            "skills_sh_normalize",
            PipelinePhase::Normalizing,
            documents.len(),
        ),
        data: documents,
    })
}

pub(crate) fn artifact_candidates(
    plan: &SourcePlan,
    generation: &SourceGenerationId,
    documents: &[SourceDocument],
) -> Result<Vec<ArtifactCandidate>> {
    documents
        .iter()
        .map(|document| candidate_from_document(plan, generation, document))
        .collect()
}

fn candidate_from_document(
    plan: &SourcePlan,
    generation: &SourceGenerationId,
    document: &SourceDocument,
) -> Result<ArtifactCandidate> {
    let payload = document.structured_payload.as_ref().ok_or_else(|| {
        ApiError::new(
            "adapter.skills_sh.structured_payload_missing",
            ErrorStage::Enriching,
            "skills.sh SourceDocument is missing its structured listing payload",
        )
    })?;
    let skill: SkillsShSkill = serde_json::from_value(payload.clone()).map_err(|error| {
        ApiError::new(
            "adapter.skills_sh.structured_payload_invalid",
            ErrorStage::Enriching,
            error.to_string(),
        )
    })?;
    let (canonical_source_uri, repository, mut warnings) = canonical_source_pointer(&skill);
    let source_digest = normalized_sha256(skill.hash.as_deref());
    let dedupe =
        artifact_candidate_dedupe(&canonical_source_uri, None, None, source_digest.as_deref());
    let mut manifest_metadata = MetadataMap::new();
    manifest_metadata.insert(
        "axonSourceItemKey".to_string(),
        serde_json::json!(document.source_item_key.0),
    );
    manifest_metadata.insert("skillsShId".to_string(), serde_json::json!(skill.id));
    manifest_metadata.insert("slug".to_string(), serde_json::json!(skill.slug));
    manifest_metadata.insert("name".to_string(), serde_json::json!(skill.name));
    manifest_metadata.insert(
        "sourceType".to_string(),
        serde_json::json!(skill.source_type),
    );
    manifest_metadata.insert(
        "axonDedupe".to_string(),
        serde_json::json!({
            "identityKey": dedupe.identity_key,
            "contentKey": dedupe.content_key,
        }),
    );
    let mut discovery_evidence = MetadataMap::new();
    discovery_evidence.insert(
        "skillsSh".to_string(),
        serde_json::json!({
            "id": skill.id,
            "url": skill.url,
            "source": skill.source,
            "sourceType": skill.source_type,
            "installUrl": skill.install_url,
            "isDuplicate": skill.is_duplicate,
        }),
    );
    let mut popularity_signals = MetadataMap::new();
    popularity_signals.insert(
        "skillsShInstalls".to_string(),
        serde_json::json!(skill.installs),
    );
    let mut license_evidence = MetadataMap::new();
    license_evidence.insert("redistribution".to_string(), serde_json::json!("unknown"));
    license_evidence.insert(
        "source".to_string(),
        serde_json::json!("canonical repository license unresolved"),
    );
    if source_digest.is_none() && skill.hash.is_some() {
        warnings.push(
            "skills.sh hash was not a canonical SHA-256 digest and was not trusted".to_string(),
        );
    }
    Ok(ArtifactCandidate {
        schema_version: ARTIFACT_CANDIDATE_SCHEMA_VERSION.to_string(),
        id: artifact_candidate_id(&dedupe),
        canonical_source_uri,
        source_provider: "axon".to_string(),
        observed_at: catalog_observed_at(document)?,
        repository,
        source_ref: None,
        source_path: None,
        kind_hints: vec!["skill".to_string()],
        observed_files: Vec::new(),
        manifest_metadata,
        content_digests: source_digest.into_iter().collect(),
        discovery_evidence,
        popularity_signals,
        license_evidence,
        crawl_generation_id: Some(generation.0.clone()),
        crawl_job_id: Some(plan.job_id.0.to_string()),
        warnings,
    })
}

fn catalog_observed_at(document: &SourceDocument) -> Result<Timestamp> {
    document
        .metadata
        .get("catalog_observed_at")
        .and_then(serde_json::Value::as_str)
        .map(|value| Timestamp(value.to_string()))
        .ok_or_else(|| {
            ApiError::new(
                "adapter.skills_sh.observed_at_missing",
                ErrorStage::Enriching,
                "skills.sh candidate is missing the catalog observation timestamp",
            )
        })
}

fn source_item_key(plan: &SourcePlan, skill: &SkillsShSkill) -> Result<SourceItemKey> {
    Ok(item_identity(
        SourceKind::Registry,
        &plan.route.source.canonical_uri,
        &skill.id,
    )?
    .source_item_key)
}

fn safe_listing_payload(skill: &SkillsShSkill) -> Result<serde_json::Value> {
    serde_json::to_value(skill).map_err(|error| {
        ApiError::new(
            "adapter.skills_sh.payload_serialize_failed",
            ErrorStage::Normalizing,
            error.to_string(),
        )
    })
}

fn listing_digest(skill: &SkillsShSkill) -> Result<String> {
    let bytes = serde_json::to_vec(skill).map_err(|error| {
        ApiError::new(
            "adapter.skills_sh.digest_serialize_failed",
            ErrorStage::Discovering,
            error.to_string(),
        )
    })?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn normalized_sha256(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    let hex = value.strip_prefix("sha256:").unwrap_or(value);
    (hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| format!("sha256:{}", hex.to_ascii_lowercase()))
}

fn canonical_source_pointer(skill: &SkillsShSkill) -> (String, Option<String>, Vec<String>) {
    if let Some(install_url) = skill.install_url.as_deref()
        && let Ok(mut url) = Url::parse(install_url)
        && matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some()
    {
        url.set_query(None);
        url.set_fragment(None);
        let canonical = url
            .as_str()
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .to_string();
        let repository = (skill.source_type.eq_ignore_ascii_case("github")
            && valid_repository_name(&skill.source))
        .then(|| skill.source.clone());
        return (canonical, repository, Vec::new());
    }
    let fallback = skill
        .url
        .clone()
        .unwrap_or_else(|| format!("https://skills.sh/{}", skill.id));
    (
        fallback,
        None,
        vec![
            "canonical source pointer could not be resolved from skills.sh installUrl; aggregator URL retained as evidence"
                .to_string(),
        ],
    )
}

fn valid_repository_name(value: &str) -> bool {
    let mut parts = value.split('/');
    matches!((parts.next(), parts.next(), parts.next()), (Some(owner), Some(repo), None) if !owner.is_empty() && !repo.is_empty())
}

fn digest_id(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
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
            hash: None,
        };
        let (uri, repo, warnings) = canonical_source_pointer(&skill);
        assert_eq!(uri, "https://github.com/vercel-labs/skills");
        assert_eq!(repo.as_deref(), Some("vercel-labs/skills"));
        assert!(warnings.is_empty());
    }

    #[test]
    fn sha256_normalization_is_fail_closed() {
        assert_eq!(
            normalized_sha256(Some(&"a".repeat(64))),
            Some(format!("sha256:{}", "a".repeat(64)))
        );
        assert!(normalized_sha256(Some("not-a-digest")).is_none());
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
}
