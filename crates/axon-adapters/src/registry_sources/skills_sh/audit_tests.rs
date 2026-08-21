use super::*;
use axon_core::http::LoopbackGuard;
use httpmock::prelude::*;
use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Debug)]
enum FakeOutcome {
    Response(Option<SkillsShAuditResponse>),
    Error(&'static str),
}

struct FakeAuditProvider {
    outcomes: Mutex<VecDeque<FakeOutcome>>,
    calls: Mutex<Vec<String>>,
}

impl FakeAuditProvider {
    fn new(outcomes: Vec<FakeOutcome>) -> Self {
        Self {
            outcomes: Mutex::new(outcomes.into()),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls mutex poisoned").clone()
    }
}

#[async_trait]
impl SkillsShAuditProvider for FakeAuditProvider {
    async fn fetch_audit(&self, id: &str) -> Result<Option<SkillsShAuditResponse>> {
        self.calls
            .lock()
            .expect("calls mutex poisoned")
            .push(id.to_string());
        match self
            .outcomes
            .lock()
            .expect("outcomes mutex poisoned")
            .pop_front()
            .expect("fake outcome exhausted")
        {
            FakeOutcome::Response(response) => Ok(response),
            FakeOutcome::Error(code) => Err(ApiError::new(
                code,
                ErrorStage::Fetching,
                "synthetic audit provider failure",
            )),
        }
    }
}

fn skill(id: &str) -> super::super::SkillsShSkill {
    let slug = id.rsplit('/').next().expect("skill id has slug");
    let source = id
        .strip_suffix(&format!("/{slug}"))
        .expect("skill id has source");
    super::super::SkillsShSkill {
        id: id.to_string(),
        slug: slug.to_string(),
        name: slug.to_string(),
        source: source.to_string(),
        installs: 1,
        source_type: "github".to_string(),
        install_url: Some(format!("https://github.com/{source}")),
        url: Some(format!("https://skills.sh/{id}")),
        is_duplicate: None,
        audits: Vec::new(),
        audit_status: None,
        audit_warnings: Vec::new(),
    }
}

fn audit_response(id: &str, status: &str) -> SkillsShAuditResponse {
    let item = skill(id);
    SkillsShAuditResponse {
        id: item.id,
        source: item.source,
        slug: item.slug,
        audits: vec![SkillsShAudit {
            provider: "Socket".to_string(),
            slug: "socket".to_string(),
            status: status.to_string(),
            summary: "No alerts".to_string(),
            audited_at: "2026-08-19T14:00:00Z".to_string(),
            risk_level: Some("LOW".to_string()),
            categories: Vec::new(),
        }],
    }
}

fn dump(ids: &[&str]) -> SkillsShDump {
    SkillsShDump {
        provider: "skills.sh".to_string(),
        observed_at: axon_api::source::Timestamp("2026-08-19T14:00:00Z".to_string()),
        skills: ids.iter().map(|id| skill(id)).collect(),
    }
}

#[test]
fn audit_url_uses_stable_id_segments_and_rejects_traversal() {
    let url = audit_url_for_test(
        "https://skills.sh/api/v1/skills",
        "vercel-labs/skills/find-skills",
    )
    .expect("valid stable id");
    assert_eq!(
        url.as_str(),
        "https://skills.sh/api/v1/skills/audit/vercel-labs/skills/find-skills"
    );
    assert!(audit_url_for_test("https://skills.sh/api/v1/skills", "acme/../secret").is_err());
}

#[tokio::test]
async fn audit_enrichment_is_opt_in_bounded_and_sequential() {
    let provider = FakeAuditProvider::new(vec![FakeOutcome::Response(Some(audit_response(
        "acme/skills/a",
        "pass",
    )))]);
    let mut dump = dump(&["acme/skills/a", "acme/skills/b", "acme/skills/c"]);

    enrich_dump_with_provider(&mut dump, 1, &provider).await;

    assert_eq!(provider.calls(), vec!["acme/skills/a"]);
    assert_eq!(dump.skills[0].audit_status.as_deref(), Some("available"));
    assert_eq!(dump.skills[0].audits.len(), 1);
    assert!(dump.skills[1].audit_status.is_none());
    assert!(dump.skills[2].audit_status.is_none());
}

#[tokio::test]
async fn audit_failure_is_fail_soft_and_stops_remaining_fanout() {
    let provider = FakeAuditProvider::new(vec![
        FakeOutcome::Response(Some(audit_response("acme/skills/a", "pass"))),
        FakeOutcome::Error("adapter.skills_sh.audit_rate_limited"),
    ]);
    let mut dump = dump(&["acme/skills/a", "acme/skills/b", "acme/skills/c"]);

    enrich_dump_with_provider(&mut dump, 3, &provider).await;

    assert_eq!(provider.calls(), vec!["acme/skills/a", "acme/skills/b"]);
    assert_eq!(dump.skills[0].audit_status.as_deref(), Some("available"));
    assert_eq!(dump.skills[1].audit_status.as_deref(), Some("unavailable"));
    assert_eq!(
        dump.skills[2].audit_status.as_deref(),
        Some("skipped_after_failure")
    );
    assert_eq!(dump.skills[1].audit_warnings.len(), 1);
}

#[tokio::test]
async fn invalid_audit_shape_is_not_persisted() {
    let provider = FakeAuditProvider::new(vec![FakeOutcome::Response(Some(audit_response(
        "acme/skills/a",
        "unknown",
    )))]);
    let mut dump = dump(&["acme/skills/a"]);

    enrich_dump_with_provider(&mut dump, 1, &provider).await;

    assert!(dump.skills[0].audits.is_empty());
    assert_eq!(dump.skills[0].audit_status.as_deref(), Some("unavailable"));
    assert!(dump.skills[0].audit_warnings[0].contains("audit_shape_invalid"));
}

#[tokio::test]
async fn http_404_is_no_audit_and_429_is_retryable_without_hidden_retry() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let no_audit = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/skills/audit/acme/skills/a")
            .header("authorization", "Bearer test-token");
        then.status(404);
    });
    let limited = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/skills/audit/acme/skills/b")
            .header("authorization", "Bearer test-token");
        then.status(429).header("Retry-After", "999");
    });
    let provider = http_provider_for_test(
        Url::parse(&server.url("/api/v1/skills")).expect("mock URL"),
        "test-token",
    )
    .expect("mock audit client");

    assert!(
        provider
            .fetch_audit("acme/skills/a")
            .await
            .expect("404 is not an error")
            .is_none()
    );
    let error = provider
        .fetch_audit("acme/skills/b")
        .await
        .expect_err("429 is surfaced to bounded enrichment");

    no_audit.assert_calls(1);
    limited.assert_calls(1);
    assert_eq!(error.code.0, "adapter.skills_sh.audit_rate_limited");
    assert!(error.retryable);
    assert_eq!(error.retry_after_ms, Some(300_000));
    assert_eq!(error.provider_id.as_deref(), Some("skills.sh"));
}
