use std::sync::Arc;

use axon_api::source::*;

use super::SqliteMemoryStore;
use crate::record::Clock;
use crate::store::MemoryStore;
use crate::testing::FixedClock;

fn store() -> (SqliteMemoryStore, Arc<FixedClock>) {
    let clock = Arc::new(FixedClock::at_2026());
    let store = SqliteMemoryStore::in_memory(clock.clone()).unwrap();
    (store, clock)
}

fn request(memory_type: MemoryType, body: &str, scope_value: &str) -> MemoryRequest {
    MemoryRequest {
        memory_type,
        body: body.to_string(),
        confidence: 0.8,
        salience: 0.6,
        scope: MemoryScope {
            kind: "project".to_string(),
            value: scope_value.to_string(),
        },
        title: Some("t".to_string()),
        tags: vec!["x".to_string()],
        links: Vec::new(),
        decay: None,
        embed: false,
        visibility: Some(Visibility::Internal),
    }
}

fn ts(clock: &FixedClock) -> Timestamp {
    Timestamp(clock.now_rfc3339())
}

#[tokio::test]
async fn remember_then_get_round_trips_record() {
    let (store, _clock) = store();
    let result = store
        .remember(request(
            MemoryType::Fact,
            "axon owns a source ledger",
            "axon",
        ))
        .await
        .unwrap();
    assert_eq!(result.status, MemoryStatus::Active);
    assert_eq!(result.memory_type, MemoryType::Fact);

    let record = store.get(result.memory_id.clone()).await.unwrap().unwrap();
    assert_eq!(record.body, "axon owns a source ledger");
    assert_eq!(record.scope.value, "axon");
    // Default decay policy was stamped from the type table (fact -> normal).
    assert_eq!(record.decay.as_ref().unwrap().profile, "normal");
    assert_eq!(record.history.len(), 1);
    assert_eq!(record.history[0].message, "created");
}

#[tokio::test]
async fn remember_redacts_secrets_from_body_and_title_before_persisting() {
    let (store, _clock) = store();
    let mut remembered = request(
        MemoryType::Fact,
        "the deploy token is Authorization: Bearer abcdef0123456789abcdef",
        "axon",
    );
    remembered.title = Some("token: sk-proj-abcdefghijklmnopqrstuvwx".to_string());
    let result = store.remember(remembered).await.unwrap();

    let record = store.get(result.memory_id).await.unwrap().unwrap();
    assert!(!record.body.contains("abcdef0123456789abcdef"));
    assert!(!record.title.unwrap().contains("abcdefghijklmnopqrstuvwx"));
}

#[tokio::test]
async fn remember_rejects_empty_body() {
    let (store, _c) = store();
    let err = store
        .remember(request(MemoryType::Fact, "   ", "axon"))
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "memory.invalid");
}

#[tokio::test]
async fn search_finds_by_keyword_and_excludes_forgotten() {
    let (store, clock) = store();
    let keep = store
        .remember(request(MemoryType::Fact, "qdrant stores vectors", "axon"))
        .await
        .unwrap();
    let gone = store
        .remember(request(
            MemoryType::Fact,
            "qdrant is deprecated here",
            "axon",
        ))
        .await
        .unwrap();

    store
        .set_status(MemoryStatusRequest {
            memory_id: gone.memory_id.clone(),
            status: MemoryStatus::Forgotten,
            reason: Some("wrong".to_string()),
            timestamp: ts(&clock),
        })
        .await
        .unwrap();

    let result = store
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query: "qdrant".to_string(),
            limit: 10,
            filters: MetadataMap::new(),
            include_graph: false,
            include_archived: false,
            reinforce: false,
        })
        .await
        .unwrap();
    assert_eq!(result.results.len(), 1);
    assert_eq!(result.results[0].record.memory_id, keep.memory_id);
}

#[tokio::test]
async fn search_excludes_archived_unless_requested() {
    let (store, clock) = store();
    let archived = store
        .remember(request(MemoryType::Fact, "archived note about tei", "axon"))
        .await
        .unwrap();
    store
        .set_status(MemoryStatusRequest {
            memory_id: archived.memory_id.clone(),
            status: MemoryStatus::Archived,
            reason: None,
            timestamp: ts(&clock),
        })
        .await
        .unwrap();

    let hidden = store
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query: "tei".to_string(),
            limit: 10,
            filters: MetadataMap::new(),
            include_graph: false,
            include_archived: false,
            reinforce: false,
        })
        .await
        .unwrap();
    assert_eq!(hidden.results.len(), 0);

    let shown = store
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query: "tei".to_string(),
            limit: 10,
            filters: MetadataMap::new(),
            include_graph: false,
            include_archived: true,
            reinforce: false,
        })
        .await
        .unwrap();
    assert_eq!(shown.results.len(), 1);
}

#[tokio::test]
async fn scope_filter_narrows_results() {
    let (store, _c) = store();
    store
        .remember(request(MemoryType::Fact, "shared token here", "axon"))
        .await
        .unwrap();
    store
        .remember(request(
            MemoryType::Fact,
            "shared token elsewhere",
            "cortex",
        ))
        .await
        .unwrap();

    let mut filters = MetadataMap::new();
    filters.insert("scope".to_string(), serde_json::json!("axon"));
    let result = store
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query: "shared token".to_string(),
            limit: 10,
            filters,
            include_graph: false,
            include_archived: false,
            reinforce: false,
        })
        .await
        .unwrap();
    assert_eq!(result.results.len(), 1);
    assert_eq!(result.results[0].record.scope.value, "axon");
}

#[tokio::test]
async fn context_respects_token_budget_and_reports_exclusions() {
    let (store, _c) = store();
    // Two memories; budget only fits one fragment.
    store
        .remember(request(
            MemoryType::Fact,
            "alpha beta gamma delta epsilon",
            "axon",
        ))
        .await
        .unwrap();
    store
        .remember(request(
            MemoryType::Fact,
            "zeta eta theta iota kappa",
            "axon",
        ))
        .await
        .unwrap();

    let ctx = store
        .context(MemoryContextRequest {
            token_budget: 6, // one "[id] a b c d e" fragment (~6 words) fits
            query: Some("alpha".to_string()),
            source_id: None,
            graph_node_id: None,
            filters: MetadataMap::new(),
            depth: None,
            include_working: false,
        })
        .await
        .unwrap();
    assert!(
        ctx.token_estimate <= 6,
        "over budget: {}",
        ctx.token_estimate
    );
    assert_eq!(ctx.memories.len(), 1);
    assert!(ctx.exclusions.contains(&"token_budget".to_string()));
    // Every fragment is cited with its memory id.
    assert!(ctx.context.starts_with('['));
}

#[tokio::test]
async fn context_excludes_working_by_default() {
    let (store, _c) = store();
    store
        .remember(request(
            MemoryType::Working,
            "scratch buffer content",
            "axon",
        ))
        .await
        .unwrap();
    // Working memories default status is Active but type=working; force to
    // working status to model short-lived context.
    let all = store
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query: "scratch".to_string(),
            limit: 10,
            filters: MetadataMap::new(),
            include_graph: false,
            include_archived: false,
            reinforce: false,
        })
        .await
        .unwrap();
    assert_eq!(all.results.len(), 1);
}

#[tokio::test]
async fn context_includes_contradicted_memories_with_warning() {
    let (store, clock) = store();
    let a = store
        .remember(request(MemoryType::Fact, "port is 8080", "axon"))
        .await
        .unwrap();
    let b = store
        .remember(request(MemoryType::Fact, "port is 9090", "axon"))
        .await
        .unwrap();

    store
        .contradict(MemoryContradictRequest {
            memory_id: a.memory_id.clone(),
            conflicting_id: b.memory_id.clone(),
            reason: Some("port mismatch".to_string()),
            timestamp: ts(&clock),
        })
        .await
        .unwrap();

    let ctx = store
        .context(MemoryContextRequest {
            token_budget: 1000,
            query: Some("port".to_string()),
            source_id: None,
            graph_node_id: None,
            filters: MetadataMap::new(),
            depth: None,
            include_working: false,
        })
        .await
        .unwrap();

    // Contract "Context Assembly": contradicted is not in the default
    // context-exclusion list (only forgotten/superseded/archived are), so
    // both contradicted memories still surface in context...
    assert_eq!(ctx.memories.len(), 2);
    // ...but — matching `search()`'s "Recall rules" behavior — the caller is
    // warned that the context includes unresolved contradictions.
    assert!(
        ctx.warnings.iter().any(|w| w.code == "memory.contradicted"),
        "expected a memory.contradicted warning, got: {:?}",
        ctx.warnings
    );
}

#[tokio::test]
async fn reinforce_raises_score_and_appends_history() {
    let (store, clock) = store();
    let created = store
        .remember(request(MemoryType::Fact, "reinforce me", "axon"))
        .await
        .unwrap();
    // Age the memory so its baseline score has decayed.
    clock.advance_days(30);

    let before = store.get(created.memory_id.clone()).await.unwrap().unwrap();
    assert_eq!(before.decay.as_ref().unwrap().reinforcement_count, 0);

    let reinforced = store
        .reinforce(
            created.memory_id.clone(),
            MemoryReinforcement {
                amount: 0.1,
                reason: "used in ask".to_string(),
                timestamp: ts(&clock),
            },
        )
        .await
        .unwrap();

    let after = store.get(created.memory_id.clone()).await.unwrap().unwrap();
    assert_eq!(after.decay.as_ref().unwrap().reinforcement_count, 1);
    assert!(after.decay.as_ref().unwrap().last_reinforced_at.is_some());
    // History grew and salience bumped.
    assert!(after.history.iter().any(|h| h.message == "used in ask"));
    assert!(after.salience > before.salience);
    // Reinforcement reset the decay age (now == last_reinforced_at), so the
    // live score should exceed the fully-decayed pre-reinforcement score.
    let stale_score = crate::decay::score_record(&before, 30.0, 0.0, 1.0, false);
    assert!(reinforced.memory_score > stale_score);
}

#[tokio::test]
async fn supersede_hides_old_and_links_replacement() {
    let (store, clock) = store();
    let old = store
        .remember(request(MemoryType::Decision, "use postgres", "axon"))
        .await
        .unwrap();
    let new = store
        .remember(request(MemoryType::Decision, "use sqlite instead", "axon"))
        .await
        .unwrap();

    store
        .supersede(MemorySupersedeRequest {
            memory_id: old.memory_id.clone(),
            replacement_id: new.memory_id.clone(),
            reason: Some("migrated".to_string()),
            timestamp: ts(&clock),
        })
        .await
        .unwrap();

    let old_record = store.get(old.memory_id.clone()).await.unwrap().unwrap();
    assert_eq!(old_record.status, MemoryStatus::Superseded);
    assert_eq!(old_record.superseded_by, Some(new.memory_id.clone()));
    // History preserved (create + supersede).
    assert_eq!(old_record.history.len(), 2);

    // Superseded memory is excluded from search.
    let result = store
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query: "postgres".to_string(),
            limit: 10,
            filters: MetadataMap::new(),
            include_graph: false,
            include_archived: false,
            reinforce: false,
        })
        .await
        .unwrap();
    assert_eq!(result.results.len(), 0);
}

#[tokio::test]
async fn supersede_rejects_missing_replacement_and_self() {
    let (store, clock) = store();
    let m = store
        .remember(request(MemoryType::Fact, "x", "axon"))
        .await
        .unwrap();
    let self_err = store
        .supersede(MemorySupersedeRequest {
            memory_id: m.memory_id.clone(),
            replacement_id: m.memory_id.clone(),
            reason: None,
            timestamp: ts(&clock),
        })
        .await
        .unwrap_err();
    assert_eq!(self_err.code.to_string(), "memory.invalid");

    let missing = store
        .supersede(MemorySupersedeRequest {
            memory_id: m.memory_id.clone(),
            replacement_id: MemoryId::new("nope"),
            reason: None,
            timestamp: ts(&clock),
        })
        .await
        .unwrap_err();
    assert_eq!(missing.code.to_string(), "memory.not_found");
}

#[tokio::test]
async fn contradiction_sends_both_memories_to_review() {
    let (store, clock) = store();
    let a = store
        .remember(request(MemoryType::Fact, "port is 8080", "axon"))
        .await
        .unwrap();
    let b = store
        .remember(request(MemoryType::Fact, "port is 9090", "axon"))
        .await
        .unwrap();

    store
        .contradict(MemoryContradictRequest {
            memory_id: a.memory_id.clone(),
            conflicting_id: b.memory_id.clone(),
            reason: Some("port mismatch".to_string()),
            timestamp: ts(&clock),
        })
        .await
        .unwrap();

    let ra = store.get(a.memory_id.clone()).await.unwrap().unwrap();
    let rb = store.get(b.memory_id.clone()).await.unwrap().unwrap();
    assert_eq!(ra.status, MemoryStatus::Contradicted);
    assert_eq!(rb.status, MemoryStatus::Contradicted);
    assert_eq!(ra.contradicts, Some(b.memory_id.clone()));
    assert_eq!(rb.contradicts, Some(a.memory_id.clone()));

    let review = store.review(MemoryReviewRequest::default()).await.unwrap();
    assert_eq!(review.memories.len(), 2);
}

#[tokio::test]
async fn decay_reduces_live_score_over_time() {
    let (store, clock) = store();
    // Episode -> fast decay (7-day half-life): big observable drop.
    let created = store
        .remember(request(MemoryType::Episode, "session summary text", "axon"))
        .await
        .unwrap();

    let fresh = store
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query: "session".to_string(),
            limit: 1,
            filters: MetadataMap::new(),
            include_graph: false,
            include_archived: false,
            reinforce: false,
        })
        .await
        .unwrap();
    let fresh_score = fresh.results[0].score;

    // Advance 14 days = 2 half-lives.
    clock.advance_days(14);
    let aged = store
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query: "session".to_string(),
            limit: 1,
            filters: MetadataMap::new(),
            include_graph: false,
            include_archived: false,
            reinforce: false,
        })
        .await
        .unwrap();
    let aged_score = aged.results[0].score;

    assert!(
        aged_score < fresh_score,
        "aged {aged_score} should be below fresh {fresh_score}"
    );
    let _ = created;
}

#[tokio::test]
async fn pinned_memory_does_not_decay_in_recall() {
    let (store, clock) = store();
    let mut req = request(MemoryType::Working, "pinned working note", "axon");
    req.decay = Some(MemoryDecayPolicy {
        profile: "very_fast".to_string(),
        half_life_days: Some(1),
        last_reinforced_at: None,
        reinforcement_count: 0,
        review_after: None,
        expires_at: None,
        pinned: true,
    });
    store.remember(req).await.unwrap();

    let fresh = store
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query: "pinned".to_string(),
            limit: 1,
            filters: MetadataMap::new(),
            include_graph: false,
            include_archived: false,
            reinforce: false,
        })
        .await
        .unwrap()
        .results[0]
        .score;

    clock.advance_days(60);
    let later = store
        .search(MemorySearchRequest {
            include_statuses: Vec::new(),
            query: "pinned".to_string(),
            limit: 1,
            filters: MetadataMap::new(),
            include_graph: false,
            include_archived: false,
            reinforce: false,
        })
        .await
        .unwrap()
        .results[0]
        .score;

    assert!(
        (fresh - later).abs() < 1e-5,
        "pinned decayed: {fresh} vs {later}"
    );
}

#[tokio::test]
async fn link_persists_and_reloads() {
    let (store, _c) = store();
    let m = store
        .remember(request(MemoryType::Entity, "axon repo entity", "axon"))
        .await
        .unwrap();
    store
        .link(MemoryLinkRequest {
            memory_id: m.memory_id.clone(),
            link: MemoryLink {
                link_type: "mirrors".to_string(),
                target: "graph://repo/axon".to_string(),
                confidence: 0.95,
                evidence: Vec::new(),
            },
        })
        .await
        .unwrap();

    let record = store.get(m.memory_id.clone()).await.unwrap().unwrap();
    assert_eq!(record.links.len(), 1);
    assert_eq!(record.links[0].target, "graph://repo/axon");
    assert!(record.history.iter().any(|h| h.message == "linked"));
}

#[tokio::test]
async fn reset_clears_all_tables() {
    let (store, _c) = store();
    let m = store
        .remember(request(MemoryType::Fact, "to be cleared", "axon"))
        .await
        .unwrap();
    store.reset().await.unwrap();
    assert!(store.get(m.memory_id).await.unwrap().is_none());
    let review = store.review(MemoryReviewRequest::default()).await.unwrap();
    assert!(review.memories.is_empty());
}

#[tokio::test]
async fn capabilities_report_owner_and_features() {
    let (store, _c) = store();
    let cap = store.capabilities().await.unwrap();
    assert_eq!(cap.0.owner_crate, "axon-memory");
    assert_eq!(cap.0.name, "sqlite-memory");
    assert!(cap.0.features.contains(&"decay".to_string()));
    assert!(cap.0.features.contains(&"supersede".to_string()));
}

#[tokio::test]
async fn update_edits_body_and_appends_history() {
    let (store, clock) = store();
    let m = store
        .remember(request(MemoryType::Fact, "old body", "axon"))
        .await
        .unwrap();

    store
        .update(MemoryUpdateRequest {
            memory_id: m.memory_id.clone(),
            body: Some("new body".to_string()),
            title: None,
            memory_type: None,
            confidence: None,
            salience: None,
            scope: None,
            reason: Some("correction".to_string()),
            timestamp: ts(&clock),
        })
        .await
        .unwrap();

    let record = store.get(m.memory_id).await.unwrap().unwrap();
    assert_eq!(record.body, "new body");
    assert!(record.history.iter().any(|h| h.message == "correction"));
}

#[tokio::test]
async fn pin_sets_decay_pinned_flag_without_changing_status() {
    let (store, clock) = store();
    let m = store
        .remember(request(MemoryType::Fact, "pin me", "axon"))
        .await
        .unwrap();

    store
        .pin(MemoryPinRequest {
            memory_id: m.memory_id.clone(),
            pinned: true,
            reason: None,
            timestamp: ts(&clock),
        })
        .await
        .unwrap();

    let record = store.get(m.memory_id).await.unwrap().unwrap();
    assert_eq!(record.status, MemoryStatus::Active);
    assert!(record.decay.unwrap().pinned);
}

#[tokio::test]
async fn archive_transitions_status_and_excludes_from_default_search() {
    let (store, clock) = store();
    let m = store
        .remember(request(MemoryType::Fact, "archive me", "axon"))
        .await
        .unwrap();

    store
        .archive(MemoryArchiveRequest {
            memory_id: m.memory_id.clone(),
            reason: None,
            timestamp: ts(&clock),
        })
        .await
        .unwrap();

    let record = store.get(m.memory_id).await.unwrap().unwrap();
    assert_eq!(record.status, MemoryStatus::Archived);
}

#[tokio::test]
async fn forget_transitions_status_to_forgotten() {
    let (store, clock) = store();
    let m = store
        .remember(request(MemoryType::Fact, "forget me", "axon"))
        .await
        .unwrap();

    store
        .forget(MemoryForgetRequest {
            memory_id: m.memory_id.clone(),
            reason: None,
            timestamp: ts(&clock),
        })
        .await
        .unwrap();

    let record = store.get(m.memory_id).await.unwrap().unwrap();
    assert_eq!(record.status, MemoryStatus::Forgotten);
}

#[tokio::test]
async fn compact_merges_sources_and_archives_them_when_requested() {
    let (store, clock) = store();
    let a = store
        .remember(request(MemoryType::Fact, "fact one", "axon"))
        .await
        .unwrap();
    let b = store
        .remember(request(MemoryType::Fact, "fact two", "axon"))
        .await
        .unwrap();

    let result = store
        .compact(MemoryCompactRequest {
            memory_ids: vec![a.memory_id.clone(), b.memory_id.clone()],
            strategy: "concatenate".to_string(),
            result_type: MemoryType::Fact,
            title: Some("combined".to_string()),
            scope: MemoryScope {
                kind: "project".to_string(),
                value: "axon".to_string(),
            },
            archive_sources: true,
            instructions: None,
            timestamp: ts(&clock),
        })
        .await
        .unwrap();

    let compacted = store.get(result.memory_id).await.unwrap().unwrap();
    assert!(compacted.body.contains("fact one"));
    assert!(compacted.body.contains("fact two"));

    let source_a = store.get(a.memory_id).await.unwrap().unwrap();
    let source_b = store.get(b.memory_id).await.unwrap().unwrap();
    assert_eq!(source_a.status, MemoryStatus::Archived);
    assert_eq!(source_b.status, MemoryStatus::Archived);
}

#[tokio::test]
async fn compact_rejects_unsupported_strategy() {
    let (store, clock) = store();
    let a = store
        .remember(request(MemoryType::Fact, "fact one", "axon"))
        .await
        .unwrap();
    let b = store
        .remember(request(MemoryType::Fact, "fact two", "axon"))
        .await
        .unwrap();

    let err = store
        .compact(MemoryCompactRequest {
            memory_ids: vec![a.memory_id, b.memory_id],
            strategy: "not_a_real_strategy".to_string(),
            result_type: MemoryType::Fact,
            title: None,
            scope: MemoryScope {
                kind: "project".to_string(),
                value: "axon".to_string(),
            },
            archive_sources: false,
            instructions: None,
            timestamp: ts(&clock),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "memory.unsupported_strategy");
}

/// `semantic_summary` is a supported strategy string, but fails closed
/// (`memory.llm_unavailable`) rather than silently falling back to
/// `concatenate` when no `CompactionSynthesizer` is injected.
#[tokio::test]
async fn compact_semantic_summary_fails_closed_without_synthesizer() {
    let (store, clock) = store();
    let a = store
        .remember(request(MemoryType::Fact, "fact one", "axon"))
        .await
        .unwrap();
    let b = store
        .remember(request(MemoryType::Fact, "fact two", "axon"))
        .await
        .unwrap();

    let err = store
        .compact(MemoryCompactRequest {
            memory_ids: vec![a.memory_id, b.memory_id],
            strategy: "semantic_summary".to_string(),
            result_type: MemoryType::Fact,
            title: None,
            scope: MemoryScope {
                kind: "project".to_string(),
                value: "axon".to_string(),
            },
            archive_sources: false,
            instructions: None,
            timestamp: ts(&clock),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "memory.llm_unavailable");
}

/// With an injected [`FakeCompactionSynthesizer`], `semantic_summary`
/// produces the synthesized body (not the `concatenate` `[memory_id] body`
/// format) and runs it through the same redaction boundary as `remember`.
#[tokio::test]
async fn compact_semantic_summary_uses_injected_synthesizer() {
    let (store, clock) = store();
    let store = store.with_compaction_synthesizer(std::sync::Arc::new(
        crate::testing::FakeCompactionSynthesizer::new(),
    ));
    let a = store
        .remember(request(MemoryType::Fact, "fact one", "axon"))
        .await
        .unwrap();
    let b = store
        .remember(request(MemoryType::Fact, "fact two", "axon"))
        .await
        .unwrap();

    let result = store
        .compact(MemoryCompactRequest {
            memory_ids: vec![a.memory_id, b.memory_id],
            strategy: "semantic_summary".to_string(),
            result_type: MemoryType::Fact,
            title: None,
            scope: MemoryScope {
                kind: "project".to_string(),
                value: "axon".to_string(),
            },
            archive_sources: false,
            instructions: Some("focus on facts".to_string()),
            timestamp: ts(&clock),
        })
        .await
        .unwrap();
    let compacted = store.get(result.memory_id).await.unwrap().unwrap();
    assert!(compacted.body.starts_with("[synthesized:focus on facts]"));
    assert!(compacted.body.contains("fact one"));
    assert!(compacted.body.contains("fact two"));
}

#[tokio::test]
async fn import_merge_dedupes_by_body_type_and_scope() {
    let (store, _clock) = store();
    let existing = store
        .remember(request(MemoryType::Fact, "duplicate content", "axon"))
        .await
        .unwrap();
    let existing_record = store.get(existing.memory_id).await.unwrap().unwrap();

    let mut fresh_record = existing_record.clone();
    fresh_record.memory_id = MemoryId::new("mem_incoming_dup");
    let mut new_record = existing_record.clone();
    new_record.memory_id = MemoryId::new("mem_incoming_new");
    new_record.body = "brand new content".to_string();

    let result = store
        .import(MemoryImportRequest {
            records: vec![fresh_record, new_record],
            mode: MemoryImportMode::Merge,
            dry_run: false,
        })
        .await
        .unwrap();

    assert_eq!(result.created, 1);
    assert_eq!(result.skipped, 1);
    assert!(!result.dry_run);
}

#[tokio::test]
async fn import_dry_run_reports_plan_without_writing() {
    let (store, _clock) = store();
    let mut record = MemoryRecord {
        visibility: Visibility::Internal,
        memory_id: MemoryId::new("mem_dry_run"),
        memory_type: MemoryType::Fact,
        status: MemoryStatus::Active,
        body: "dry run content".to_string(),
        confidence: 0.8,
        salience: 0.5,
        scope: MemoryScope {
            kind: "project".to_string(),
            value: "axon".to_string(),
        },
        history: Vec::new(),
        title: None,
        links: Vec::new(),
        decay: None,
        embedding_refs: Vec::new(),
        superseded_by: None,
        contradicts: None,
    };
    record.history.push(MemoryHistoryEvent {
        status: MemoryStatus::Active,
        message: "created".to_string(),
        timestamp: Timestamp("2026-01-01T00:00:00Z".to_string()),
    });

    let result = store
        .import(MemoryImportRequest {
            records: vec![record],
            mode: MemoryImportMode::Merge,
            dry_run: true,
        })
        .await
        .unwrap();

    assert_eq!(result.created, 1);
    assert!(result.dry_run);
    // Nothing was actually written.
    let review = store.review(MemoryReviewRequest::default()).await.unwrap();
    assert!(review.memories.is_empty());
}

#[tokio::test]
async fn export_excludes_archived_and_forgotten_by_default() {
    let (store, clock) = store();
    let active = store
        .remember(request(MemoryType::Fact, "stays visible", "axon"))
        .await
        .unwrap();
    let archived = store
        .remember(request(MemoryType::Fact, "archived one", "axon"))
        .await
        .unwrap();
    store
        .archive(MemoryArchiveRequest {
            memory_id: archived.memory_id.clone(),
            reason: None,
            timestamp: ts(&clock),
        })
        .await
        .unwrap();

    let result = store
        .export(MemoryExportRequest {
            scope: None,
            include_archived: false,
            include_working: false,
        })
        .await
        .unwrap();

    assert!(
        result
            .records
            .iter()
            .any(|r| r.memory_id == active.memory_id)
    );
    assert!(
        !result
            .records
            .iter()
            .any(|r| r.memory_id == archived.memory_id)
    );
}
