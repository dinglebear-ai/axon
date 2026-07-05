//! New-source onboarding checklist derivation.

use crate::spec::{ParserFamily, SourceAdapterSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardingRow {
    pub complete: bool,
    pub evidence: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceOnboardingStatus {
    pub identity: OnboardingRow,
    pub resolver: OnboardingRow,
    pub router: OnboardingRow,
    pub adapter: OnboardingRow,
    pub scopes: OnboardingRow,
    pub ledger: OnboardingRow,
    pub parsing: OnboardingRow,
    pub graph: OnboardingRow,
    pub chunking: OnboardingRow,
    pub metadata: OnboardingRow,
    pub auth_secrets: OnboardingRow,
    pub observability: OnboardingRow,
    pub error_handling: OnboardingRow,
    pub tests: OnboardingRow,
    pub docs: OnboardingRow,
}

pub fn onboarding_status(spec: &SourceAdapterSpec) -> SourceOnboardingStatus {
    let has_identity = !spec.source_kinds.is_empty() && !spec.adapter.is_empty();
    let has_resolver = !spec.is_source_adapter
        || !spec.supported_schemes.is_empty()
        || !spec.shorthand_patterns.is_empty();
    let has_scope_contract = !spec.scopes.is_empty() || !spec.is_source_adapter;
    let has_parser_contract =
        !spec.parser_families.is_empty() || spec.parser_families.contains(&ParserFamily::None);
    let has_graph_contract = !spec.required_graph_fact_kinds.is_empty();
    let has_metadata_contract = !spec.metadata_families.is_empty();
    let has_error_contract = !spec.degraded_modes.is_empty();
    let has_option_schema = !spec.option_schema.is_empty();

    SourceOnboardingStatus {
        identity: row(
            has_identity,
            "source kinds, adapter name, and version declared",
        ),
        resolver: row(has_resolver, "schemes or shorthand patterns declared"),
        router: row(
            has_scope_contract,
            "default scope and scope capabilities declared",
        ),
        adapter: row(
            !spec.is_source_adapter || !spec.adapter.is_empty(),
            "adapter or integration identity declared",
        ),
        scopes: row(has_scope_contract, "scope capability rows declared"),
        ledger: row(
            !spec.is_source_adapter || has_identity,
            "stable adapter identity available for ledger item keys and generations",
        ),
        parsing: row(has_parser_contract, "parser family declarations present"),
        graph: row(
            has_graph_contract,
            "required graph fact declarations present",
        ),
        chunking: row(
            has_parser_contract,
            "parser families route to chunking profiles",
        ),
        metadata: row(has_metadata_contract, "metadata families declared"),
        auth_secrets: row(
            spec.credential_requirements.is_empty()
                || spec
                    .credential_requirements
                    .iter()
                    .all(|requirement| !requirement.reason.is_empty()),
            "credential requirements and security flags declared",
        ),
        observability: row(
            has_error_contract && has_identity,
            "degraded modes and adapter identity declared for warnings/counts",
        ),
        error_handling: row(has_error_contract, "degraded and failure modes declared"),
        tests: row(
            !spec.is_source_adapter || has_identity,
            "family participates in source-family contract tests",
        ),
        docs: row(
            has_option_schema && has_metadata_contract,
            "option schema and metadata families available to generated capability docs",
        ),
    }
}

pub fn onboarding_rows(status: &SourceOnboardingStatus) -> [&OnboardingRow; 15] {
    [
        &status.identity,
        &status.resolver,
        &status.router,
        &status.adapter,
        &status.scopes,
        &status.ledger,
        &status.parsing,
        &status.graph,
        &status.chunking,
        &status.metadata,
        &status.auth_secrets,
        &status.observability,
        &status.error_handling,
        &status.tests,
        &status.docs,
    ]
}

fn row(complete: bool, evidence: &'static str) -> OnboardingRow {
    OnboardingRow { complete, evidence }
}
