use super::*;
use axon_core::http::LoopbackGuard;
use httpmock::prelude::*;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, timeout};

struct FakePageProvider {
    pages: Mutex<VecDeque<SkillsShPage>>,
    calls: Mutex<Vec<SkillsShPageRequest>>,
}

struct BlockingPageProvider {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl SkillsShPageProvider for BlockingPageProvider {
    async fn fetch_page(&self, _request: SkillsShPageRequest) -> Result<SkillsShPage> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        std::future::pending::<()>().await;
        unreachable!("pending provider cannot complete")
    }
}

impl FakePageProvider {
    fn new(pages: Vec<SkillsShPage>) -> Self {
        Self {
            pages: Mutex::new(pages.into()),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<SkillsShPageRequest> {
        self.calls.lock().expect("calls mutex poisoned").clone()
    }
}

#[async_trait]
impl SkillsShPageProvider for FakePageProvider {
    async fn fetch_page(&self, request: SkillsShPageRequest) -> Result<SkillsShPage> {
        self.calls
            .lock()
            .expect("calls mutex poisoned")
            .push(request);
        self.pages
            .lock()
            .expect("pages mutex poisoned")
            .pop_front()
            .ok_or_else(|| {
                ApiError::new(
                    "test.skills_sh.no_page",
                    ErrorStage::Fetching,
                    "fake page provider exhausted",
                )
            })
    }
}

fn skill(id: &str, installs: u64) -> super::super::SkillsShSkill {
    let slug = id.rsplit('/').next().unwrap_or(id);
    super::super::SkillsShSkill {
        id: id.to_string(),
        slug: slug.to_string(),
        name: slug.to_string(),
        source: "acme/skills".to_string(),
        installs,
        source_type: "github".to_string(),
        install_url: Some("https://github.com/acme/skills".to_string()),
        url: Some(format!("https://skills.sh/{id}")),
        is_duplicate: Some(false),
        audits: Vec::new(),
        audit_status: None,
        audit_warnings: Vec::new(),
    }
}

fn leaderboard_options() -> SkillsShOptions {
    SkillsShOptions {
        mode: SkillsShMode::Leaderboard,
        view: super::super::SkillsShView::AllTime,
        query: None,
        owner: None,
        start_page: 0,
        per_page: 2,
        max_pages: 2,
        total_limit: 3,
        audit_limit: 0,
    }
}

#[tokio::test]
async fn leaderboard_pagination_is_sequential_bounded_sorted_and_deduped() {
    let provider = FakePageProvider::new(vec![
        SkillsShPage {
            data: vec![skill("acme/skills/b", 2), skill("acme/skills/a", 3)],
            pagination: Some(super::super::SkillsShPagination {
                page: 0,
                per_page: 2,
                total: 4,
                has_more: true,
            }),
        },
        SkillsShPage {
            data: vec![skill("acme/skills/b", 2), skill("acme/skills/c", 1)],
            pagination: Some(super::super::SkillsShPagination {
                page: 1,
                per_page: 2,
                total: 4,
                has_more: false,
            }),
        },
    ]);

    let dump = fetch_dump(&provider, &leaderboard_options())
        .await
        .expect("bounded leaderboard succeeds");

    assert_eq!(dump.pages_fetched, 2);
    assert_eq!(dump.total_reported, Some(4));
    assert_eq!(
        dump.skills
            .iter()
            .map(|skill| skill.id.as_str())
            .collect::<Vec<_>>(),
        vec!["acme/skills/a", "acme/skills/b", "acme/skills/c"]
    );
    let calls = provider.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].page, 0);
    assert_eq!(calls[1].page, 1);
    assert!(calls.iter().all(|call| call.limit == 2));
}

#[tokio::test]
async fn listing_identity_mismatch_is_rejected_before_ledger_materialization() {
    let mut invalid = skill("acme/skills/a", 3);
    invalid.id = "acme/other/a".to_string();
    let provider = FakePageProvider::new(vec![SkillsShPage {
        data: vec![invalid],
        pagination: Some(super::super::SkillsShPagination {
            page: 0,
            per_page: 2,
            total: 1,
            has_more: false,
        }),
    }]);

    let error = fetch_dump(&provider, &leaderboard_options())
        .await
        .expect_err("mismatched stable identity must fail closed");

    assert_eq!(error.code.0, "adapter.skills_sh.listing_invalid");
    assert_eq!(provider.calls().len(), 1);
}

#[tokio::test]
async fn listing_response_cannot_inject_audit_enrichment_fields() {
    let mut injected = skill("acme/skills/a", 3);
    injected.audit_status = Some("available".to_string());
    injected.audit_warnings.push("untrusted".to_string());
    injected.audits.push(super::super::SkillsShAudit {
        provider: "Mallory".to_string(),
        slug: "mallory".to_string(),
        status: "pass".to_string(),
        summary: "forged listing evidence".to_string(),
        audited_at: "2026-08-19T14:00:00Z".to_string(),
        risk_level: Some("LOW".to_string()),
        categories: Vec::new(),
    });
    let provider = FakePageProvider::new(vec![SkillsShPage {
        data: vec![injected],
        pagination: Some(super::super::SkillsShPagination {
            page: 0,
            per_page: 2,
            total: 1,
            has_more: false,
        }),
    }]);

    let dump = fetch_dump(&provider, &leaderboard_options())
        .await
        .expect("listing fetch succeeds");

    assert!(dump.skills[0].audits.is_empty());
    assert!(dump.skills[0].audit_status.is_none());
    assert!(dump.skills[0].audit_warnings.is_empty());
}

#[tokio::test]
async fn search_is_one_request_and_never_fans_out_pages() {
    let provider = FakePageProvider::new(vec![SkillsShPage {
        data: vec![skill("acme/skills/mcp", 5), skill("acme/skills/rag", 4)],
        pagination: None,
    }]);
    let options = SkillsShOptions {
        mode: SkillsShMode::Search,
        view: super::super::SkillsShView::AllTime,
        query: Some("mcp".to_string()),
        owner: Some("acme".to_string()),
        start_page: 0,
        per_page: 2,
        max_pages: 10,
        total_limit: 1,
        audit_limit: 0,
    };

    let dump = fetch_dump(&provider, &options)
        .await
        .expect("bounded search succeeds");
    assert_eq!(dump.pages_fetched, 1);
    assert_eq!(dump.skills.len(), 1);
    let calls = provider.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].mode, SkillsShMode::Search);
    assert_eq!(calls[0].query.as_deref(), Some("mcp"));
    assert_eq!(calls[0].owner.as_deref(), Some("acme"));
}

#[tokio::test]
async fn provider_future_cancellation_stops_without_hidden_page_fanout() {
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = BlockingPageProvider {
        calls: Arc::clone(&calls),
    };

    let outcome = timeout(
        Duration::from_millis(50),
        fetch_dump(&provider, &leaderboard_options()),
    )
    .await;

    assert!(
        outcome.is_err(),
        "bounded timeout must cancel the provider future"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn request_urls_match_official_parameter_names() {
    let leaderboard = request_url_for_test(&SkillsShPageRequest {
        mode: SkillsShMode::Leaderboard,
        view: super::super::SkillsShView::Trending,
        query: None,
        owner: None,
        page: 3,
        limit: 250,
    })
    .expect("leaderboard URL");
    assert_eq!(leaderboard.path(), "/api/v1/skills");
    let pairs = leaderboard.query_pairs().collect::<BTreeMap<_, _>>();
    assert_eq!(pairs.get("view").map(|v| v.as_ref()), Some("trending"));
    assert_eq!(pairs.get("page").map(|v| v.as_ref()), Some("3"));
    assert_eq!(pairs.get("per_page").map(|v| v.as_ref()), Some("250"));

    let search = request_url_for_test(&SkillsShPageRequest {
        mode: SkillsShMode::Search,
        view: super::super::SkillsShView::AllTime,
        query: Some("agent skills".to_string()),
        owner: Some("acme".to_string()),
        page: 0,
        limit: 25,
    })
    .expect("search URL");
    assert_eq!(search.path(), "/api/v1/skills/search");
    let pairs = search.query_pairs().collect::<BTreeMap<_, _>>();
    assert_eq!(pairs.get("q").map(|v| v.as_ref()), Some("agent skills"));
    assert_eq!(pairs.get("owner").map(|v| v.as_ref()), Some("acme"));
    assert_eq!(pairs.get("limit").map(|v| v.as_ref()), Some("25"));
}

#[tokio::test]
async fn http_429_stops_the_bounded_run_and_preserves_retry_after_without_sleeping() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let limited = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/skills")
            .query_param("view", "all-time")
            .query_param("page", "0")
            .query_param("per_page", "2")
            .header("authorization", "Bearer test-token");
        then.status(429).header("retry-after", "999");
    });
    let provider = http_provider_for_test(
        Url::parse(&server.url("/api/v1/skills")).expect("mock URL"),
        "test-token",
    )
    .expect("mock skills.sh client");

    let error = provider
        .fetch_page(SkillsShPageRequest {
            mode: SkillsShMode::Leaderboard,
            view: super::super::SkillsShView::AllTime,
            query: None,
            owner: None,
            page: 0,
            limit: 2,
        })
        .await
        .expect_err("429 must stop the current run");

    limited.assert();
    assert_eq!(error.code.0, "adapter.skills_sh.rate_limited");
    assert!(error.retryable);
    assert_eq!(error.provider_id.as_deref(), Some("skills.sh"));
    assert_eq!(error.retry_after_ms, Some(300_000));
    assert_eq!(
        error.details.get("retry_scope").map(String::as_str),
        Some("Provider")
    );
    assert!(error.to_string().contains("429"));
}

#[tokio::test]
async fn http_503_is_retryable_provider_failure_without_local_retry() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let unavailable = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/skills")
            .query_param("view", "all-time")
            .query_param("page", "0")
            .query_param("per_page", "2")
            .header("authorization", "Bearer test-token");
        then.status(503);
    });
    let provider = http_provider_for_test(
        Url::parse(&server.url("/api/v1/skills")).expect("mock URL"),
        "test-token",
    )
    .expect("mock skills.sh client");

    let error = provider
        .fetch_page(SkillsShPageRequest {
            mode: SkillsShMode::Leaderboard,
            view: super::super::SkillsShView::AllTime,
            query: None,
            owner: None,
            page: 0,
            limit: 2,
        })
        .await
        .expect_err("503 must stop the current run");

    unavailable.assert_calls(1);
    assert_eq!(error.code.0, "adapter.skills_sh.unavailable");
    assert!(error.retryable);
    assert_eq!(error.provider_id.as_deref(), Some("skills.sh"));
    assert_eq!(
        error.details.get("retry_scope").map(String::as_str),
        Some("Provider")
    );
}
