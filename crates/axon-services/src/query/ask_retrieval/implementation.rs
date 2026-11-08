//! Bounded implementation-oriented retrieval, retaining the unmodified question.
use axon_error::ApiError;
use axon_retrieval::{QueryServiceHit, QueryServiceRequest, QueryServiceResult};
use std::collections::HashMap;

pub(super) fn wants_implementation(question: &str) -> bool {
    let words = question
        .to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let has = |options: &[&str]| words.iter().any(|w| options.contains(&w.as_str()));
    !has(&[
        "plan",
        "plans",
        "planned",
        "planning",
        "roadmap",
        "roadmaps",
        "proposal",
        "proposals",
        "history",
        "historical",
        "wip",
        "draft",
    ]) && has(&[
        "create",
        "build",
        "implement",
        "write",
        "develop",
        "code",
        "program",
    ]) && has(&[
        "plugin",
        "extension",
        "module",
        "hook",
        "adapter",
        "api",
        "integration",
        "function",
        "component",
        "library",
        "parser",
        "provider",
        "software",
        "application",
    ])
}

pub(super) fn implementation_query(question: &str) -> Option<String> {
    wants_implementation(question)
        .then(|| format!("{question} source code implementation API module manifest example"))
}

pub(super) fn code_priority(hit: &QueryServiceHit) -> u8 {
    let path = reqwest::Url::parse(&hit.canonical_uri)
        .map(|url| url.path().to_ascii_lowercase())
        .unwrap_or_else(|_| hit.canonical_uri.to_ascii_lowercase());
    let path = path.split(['?', '#']).next().unwrap_or(&path);
    let ext = path.rsplit('.').next().unwrap_or("");
    if ![
        "rs", "ex", "exs", "py", "go", "ts", "tsx", "js", "jsx", "c", "h", "cpp", "java", "kt",
        "rb", "cs", "swift", "php",
    ]
    .contains(&ext)
    {
        return 0;
    }
    if path.contains("/test/")
        || path.contains("/tests/")
        || path.contains("_test.")
        || path.ends_with(".exs")
    {
        return 1;
    }
    if path.contains("example") || path.contains("/samples/") {
        return 3;
    }
    2
}

/// Reciprocal-rank fusion uses ranks, never adds incompatible dense/RRF scores.
pub(super) fn fuse(
    original: Vec<QueryServiceHit>,
    mut implementation: Vec<QueryServiceHit>,
    limit: usize,
) -> Vec<QueryServiceHit> {
    implementation.sort_by_key(|hit| std::cmp::Reverse(code_priority(hit)));
    let mut merged: HashMap<String, (QueryServiceHit, f64, usize)> = HashMap::new();
    for (arm, hits) in [original, implementation].into_iter().enumerate() {
        for (rank, hit) in hits.into_iter().enumerate() {
            let score = 1.0 / (60.0 + rank as f64 + 1.0);
            let tie = rank * 2 + arm;
            let entry = merged
                .entry(hit.chunk_id.clone())
                .or_insert((hit, 0.0, tie));
            entry.1 += score;
            entry.2 = entry.2.min(tie);
        }
    }
    let mut hits = merged.into_values().collect::<Vec<_>>();
    hits.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.2.cmp(&b.2)));
    // Documents present in both arms earn two votes. Do not let that crowd
    // every once-only implementation out before the context selector sees it.
    // Retain the best fused hit, and at most one example and production file.
    let mut selected = (0..hits.len().min(limit)).collect::<Vec<_>>();
    let mut reserved = 1;
    for priority in [3, 2] {
        if reserved >= selected.len() {
            break;
        }
        if let Some(index) = hits
            .iter()
            .position(|(hit, _, _)| code_priority(hit) == priority)
        {
            if !selected.contains(&index) {
                let slot = selected.len() - reserved;
                selected[slot] = index;
            }
            reserved += 1;
        }
    }
    selected.sort_unstable();
    hits.into_iter()
        .enumerate()
        .filter(|(index, _)| selected.binary_search(index).is_ok())
        .map(|(_, hit)| hit)
        .map(|(mut hit, score, _)| {
            hit.score = score;
            hit
        })
        .collect()
}

pub(super) async fn retrieve<F, Fut>(
    request: QueryServiceRequest,
    min_relevance: f64,
    mut run: F,
) -> Result<QueryServiceResult, ApiError>
where
    F: FnMut(QueryServiceRequest) -> Fut,
    Fut: std::future::Future<Output = Result<QueryServiceResult, ApiError>>,
{
    let extra = implementation_query(&request.query);
    let original = run(request.clone()).await?;
    let Some(query) = extra else {
        return Ok(original);
    };
    let mut secondary = request.clone();
    secondary.query = query;
    // The same authorization, kinds, dates, candidate and provider budgets apply.
    let secondary = run(secondary).await.map_err(|mut error| {
        if !original.warnings.is_empty() {
            error.details.insert(
                "original_retrieval_warnings".into(),
                serde_json::to_string(&original.warnings)
                    .expect("source warnings contain only JSON-serializable fields"),
            );
        }
        error.details.insert("retrieval_arm".into(), "implementation".into());
        error.details.insert("recovery".into(), "Inspect the embedding/vector provider error; retry after provider recovery. Original retrieval succeeded but no partial answer was generated.".into());
        error
    })?;
    // Preserve the configured cosine floor before replacing scores with RRF.
    // Hybrid arm scores already use RRF and cannot use a cosine threshold.
    let admissible = |hits: Vec<QueryServiceHit>| {
        hits.into_iter()
            .filter(|hit| request.hybrid || hit.score >= min_relevance)
            .collect()
    };
    let mut warnings = original.warnings;
    warnings.extend(secondary.warnings);
    Ok(QueryServiceResult {
        warnings,
        hits: fuse(
            admissible(original.hits),
            admissible(secondary.hits),
            request.limit as usize,
        ),
    })
}

fn is_planning_artifact(hit: &QueryServiceHit) -> bool {
    let path = reqwest::Url::parse(&hit.canonical_uri)
        .map(|url| url.path().to_ascii_lowercase())
        .unwrap_or_else(|_| hit.canonical_uri.to_ascii_lowercase());
    path.split('/').any(|segment| {
        matches!(
            segment,
            ".wip" | "wip" | "plans" | "planning" | "roadmap" | "roadmaps" | "proposals" | "drafts"
        ) || matches!(segment, "roadmap.md" | "plan.md" | "proposal.md")
    })
}

pub(super) fn document_diverse_ranked_order(ranked: &super::ranking::RankingResult) -> Vec<usize> {
    let mut seen_documents = std::collections::BTreeSet::new();
    let mut first_chunks = Vec::with_capacity(ranked.ranked_indices.len());
    let mut repeated_chunks = Vec::new();
    // A plan is useful evidence about intent, not authority over a matching
    // shipped implementation. Only suppress explicitly marked planning paths
    // when this implementation request has admitted actual non-planning code.
    let has_implementation = ranked.implementation_intent
        && ranked.ranked_indices.iter().any(|i| {
            let hit = &ranked.candidates[*i].hit;
            code_priority(hit) >= 2 && !is_planning_artifact(hit)
        });
    for index in ranked.ranked_indices.iter().copied() {
        if has_implementation && is_planning_artifact(&ranked.candidates[index].hit) {
            continue;
        }
        let identity = super::normalize::canonical_source_identity(
            &ranked.candidates[index].hit.canonical_uri,
        );
        if seen_documents.insert(identity) {
            first_chunks.push(index);
        } else {
            repeated_chunks.push(index);
        }
    }
    if ranked.implementation_intent {
        let mut code = first_chunks
            .iter()
            .copied()
            .filter(|i| code_priority(&ranked.candidates[*i].hit) > 0)
            .collect::<Vec<_>>();
        code.sort_by_key(|i| std::cmp::Reverse(code_priority(&ranked.candidates[*i].hit)));
        let mut preferred = Vec::new();
        // Keep the highest relevance source, plus one example and one production implementation.
        if let Some(index) = first_chunks.first() {
            preferred.push(*index);
        }
        for priority in [3, 2] {
            if let Some(index) = code
                .iter()
                .find(|i| code_priority(&ranked.candidates[**i].hit) == priority)
            {
                if !preferred.contains(index) {
                    preferred.push(*index);
                }
            }
        }
        for index in first_chunks {
            if !preferred.contains(&index) {
                preferred.push(index);
            }
        }
        first_chunks = preferred;
    }
    first_chunks.extend(repeated_chunks);
    first_chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::ask_retrieval::tests::citation;
    fn warning(code: &str) -> axon_api::source::SourceWarning {
        axon_api::source::SourceWarning {
            code: code.into(),
            severity: axon_api::source::Severity::Warning,
            message: "retrieval provider reported degraded results".into(),
            source_item_key: None,
            retryable: true,
        }
    }
    fn hit(id: &str, uri: &str) -> QueryServiceHit {
        QueryServiceHit {
            canonical_uri: uri.into(),
            chunk_id: id.into(),
            score: 0.8,
            text: "extension implementation".into(),
            citation: citation(uri),
        }
    }
    #[tokio::test]
    async fn two_arm_retrieval_preserves_scope_versions_and_dense_floor() {
        let request = QueryServiceRequest {
            query: "How do I create a Product 8 extension?".into(),
            collection: "owned".into(),
            limit: 5,
            hybrid: false,
            hybrid_candidates: None,
            source_kinds: vec![axon_api::SourceKind::Git],
            source_urls: vec!["https://host.invalid/examples/main.rs".into()],
            since: Some("2026-01-01T00:00:00Z".into()),
            before: None,
        };
        let mut calls = Vec::new();
        let result = retrieve(request.clone(), 0.3, |request| {
            calls.push(request);
            let hits = if calls.len() == 1 {
                (0..20)
                    .map(|n| {
                        hit(
                            &format!("doc-{n}"),
                            &format!("https://host.invalid/plan-{n}.md"),
                        )
                    })
                    .collect()
            } else {
                let mut weak = hit("unrelated", "https://host.invalid/examples/unrelated.rs");
                weak.score = 0.1;
                vec![
                    hit("implementation", "https://host.invalid/examples/main.rs"),
                    weak,
                ]
            };
            std::future::ready(Ok(QueryServiceResult {
                warnings: vec![warning(if calls.len() == 1 {
                    "original"
                } else {
                    "secondary"
                })],
                hits,
            }))
        })
        .await
        .unwrap();
        assert_eq!(
            result
                .warnings
                .iter()
                .map(|w| w.code.as_str())
                .collect::<Vec<_>>(),
            vec!["original", "secondary"]
        );
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], request);
        assert!(calls[1].query.starts_with(&request.query));
        assert_eq!(calls[1].source_kinds, request.source_kinds);
        assert_eq!(calls[1].source_urls, request.source_urls);
        assert_eq!(calls[1].since, request.since);
        assert_eq!(calls[1].limit, request.limit);
        assert!(result.hits.iter().any(|h| h.chunk_id == "implementation"));
        assert!(!result.hits.iter().any(|h| h.chunk_id == "unrelated"));
        calls.clear();
        let mut conceptual = request;
        conceptual.query = "What is a Product 8 extension?".into();
        let result = retrieve(conceptual.clone(), 0.3, |request| {
            calls.push(request);
            std::future::ready(Ok(QueryServiceResult {
                warnings: vec![warning("conceptual")],
                hits: vec![],
            }))
        })
        .await
        .unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(result.warnings[0].code, "conceptual");
        let mut calls = 0;
        let mut request = conceptual;
        request.query = "How do I create a Product 8 extension?".into();
        let error = retrieve(request, 0.3, |_| {
            calls += 1;
            std::future::ready(if calls == 1 {
                Ok(QueryServiceResult {
                    warnings: vec![warning("original")],
                    hits: vec![],
                })
            } else {
                Err(ApiError::new(
                    "provider.unavailable",
                    axon_error::ErrorStage::Retrieving,
                    "secondary provider unavailable",
                ))
            })
        })
        .await
        .unwrap_err();
        assert_eq!(error.details["retrieval_arm"], "implementation");
        let warnings: Vec<axon_api::source::SourceWarning> =
            serde_json::from_str(&error.details["original_retrieval_warnings"]).unwrap();
        assert_eq!(warnings[0].code, "original");
    }
    #[test]
    fn code_classification_uses_path_not_hostname_and_keeps_tests_distinct() {
        assert_eq!(
            code_priority(&hit("production", "https://example.com/lib/main.rs")),
            2
        );
        assert_eq!(
            code_priority(&hit("example", "https://host.invalid/examples/main.rs")),
            3
        );
        assert_eq!(
            code_priority(&hit("test", "https://host.invalid/examples/tests/main.rs")),
            1
        );
    }
    #[test]
    fn implementation_query_preserves_original_version_and_has_no_product_specific_rewrite() {
        let q = "How do I create a Product 8 extension?";
        assert!(implementation_query(q).unwrap().starts_with(q));
        assert!(implementation_query("What is a Product 8 extension?").is_none());
        assert!(implementation_query("How do I create a storage array?").is_none());
    }
    #[test]
    fn fusion_admits_example_outside_original_window_and_deduplicates_chunks() {
        let original = (0..20)
            .map(|n| {
                hit(
                    &format!("doc-{n}"),
                    &format!("https://example.com/plan-{n}.md"),
                )
            })
            .collect();
        let mut secondary = (0..20)
            .map(|n| {
                hit(
                    &format!("doc-{n}"),
                    &format!("https://example.com/plan-{n}.md"),
                )
            })
            .collect::<Vec<_>>();
        secondary.push(hit(
            "working-example",
            "https://example.com/examples/main.rs",
        ));
        let fused = fuse(original, secondary, 5);
        assert!(fused.iter().any(|h| h.chunk_id == "working-example"));
        assert_eq!(fused.len(), 5);
        assert_eq!(fused.iter().filter(|h| h.chunk_id == "doc-0").count(), 1);
    }
}
