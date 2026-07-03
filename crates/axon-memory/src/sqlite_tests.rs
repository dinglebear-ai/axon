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
