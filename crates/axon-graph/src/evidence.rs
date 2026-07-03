//! Closed `EvidenceKind` registry for the SourceGraph.
//!
//! The canonical evidence-kind list is defined in
//! `docs/pipeline-unification/sources/source-graph.md` ("Evidence Kinds").
//! Evidence kinds classify *why* a node/edge claim exists; the merge/authority
//! logic uses [`EvidenceKind::authority`] to rank competing claims.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::authority::Authority;
use crate::error::graph_validation_error;

/// A closed evidence kind from the canonical SourceGraph registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    UserPinned,
    Redirect,
    HtmlCanonical,
    Sitemap,
    Robots,
    LlmsTxt,
    GithubHomepage,
    GithubTopics,
    PackageRepository,
    PackageHomepage,
    DependencyManifest,
    DependencyLockfile,
    ContainerManifest,
    RuntimeManifest,
    EnvExample,
    ApiSchema,
    FrameworkRoute,
    CiWorkflow,
    ToolchainManifest,
    DocsLinkback,
    LocalGitRemote,
    LocalGitCommit,
    SessionMetadata,
    SessionJsonl,
    SessionJson,
    AgentInvocationEvent,
    ToolCallEvent,
    ToolResultEvent,
    SkillInvocationEvent,
    ConversationReference,
    TextMention,
    DerivedSourceAttribution,
}

impl EvidenceKind {
    /// Every evidence kind in registry order.
    pub const ALL: &'static [EvidenceKind] = &[
        Self::UserPinned,
        Self::Redirect,
        Self::HtmlCanonical,
        Self::Sitemap,
        Self::Robots,
        Self::LlmsTxt,
        Self::GithubHomepage,
        Self::GithubTopics,
        Self::PackageRepository,
        Self::PackageHomepage,
        Self::DependencyManifest,
        Self::DependencyLockfile,
        Self::ContainerManifest,
        Self::RuntimeManifest,
        Self::EnvExample,
        Self::ApiSchema,
        Self::FrameworkRoute,
        Self::CiWorkflow,
        Self::ToolchainManifest,
        Self::DocsLinkback,
        Self::LocalGitRemote,
        Self::LocalGitCommit,
        Self::SessionMetadata,
        Self::SessionJsonl,
        Self::SessionJson,
        Self::AgentInvocationEvent,
        Self::ToolCallEvent,
        Self::ToolResultEvent,
        Self::SkillInvocationEvent,
        Self::ConversationReference,
        Self::TextMention,
        Self::DerivedSourceAttribution,
    ];

    /// The exact registry name (snake_case) for this evidence kind.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserPinned => "user_pinned",
            Self::Redirect => "redirect",
            Self::HtmlCanonical => "html_canonical",
            Self::Sitemap => "sitemap",
            Self::Robots => "robots",
            Self::LlmsTxt => "llms_txt",
            Self::GithubHomepage => "github_homepage",
            Self::GithubTopics => "github_topics",
            Self::PackageRepository => "package_repository",
            Self::PackageHomepage => "package_homepage",
            Self::DependencyManifest => "dependency_manifest",
            Self::DependencyLockfile => "dependency_lockfile",
            Self::ContainerManifest => "container_manifest",
            Self::RuntimeManifest => "runtime_manifest",
            Self::EnvExample => "env_example",
            Self::ApiSchema => "api_schema",
            Self::FrameworkRoute => "framework_route",
            Self::CiWorkflow => "ci_workflow",
            Self::ToolchainManifest => "toolchain_manifest",
            Self::DocsLinkback => "docs_linkback",
            Self::LocalGitRemote => "local_git_remote",
            Self::LocalGitCommit => "local_git_commit",
            Self::SessionMetadata => "session_metadata",
            Self::SessionJsonl => "session_jsonl",
            Self::SessionJson => "session_json",
            Self::AgentInvocationEvent => "agent_invocation_event",
            Self::ToolCallEvent => "tool_call_event",
            Self::ToolResultEvent => "tool_result_event",
            Self::SkillInvocationEvent => "skill_invocation_event",
            Self::ConversationReference => "conversation_reference",
            Self::TextMention => "text_mention",
            Self::DerivedSourceAttribution => "derived_source_attribution",
        }
    }

    /// The authority this evidence kind confers on a claim.
    ///
    /// Encodes the conflict rules from source-graph.md:
    /// - user-pinned mappings are `UserPinned` authority
    /// - official package/repo/site metadata outranks community/derived
    ///   (`Official`)
    /// - derived-source attribution is `Mirror`
    /// - low-confidence text mentions are `Inferred` and must not create
    ///   authoritative edges
    pub const fn authority(self) -> Authority {
        match self {
            Self::UserPinned => Authority::UserPinned,
            Self::GithubHomepage
            | Self::GithubTopics
            | Self::PackageRepository
            | Self::PackageHomepage
            | Self::ApiSchema => Authority::Official,
            Self::Redirect
            | Self::HtmlCanonical
            | Self::Sitemap
            | Self::Robots
            | Self::LlmsTxt
            | Self::DependencyManifest
            | Self::DependencyLockfile
            | Self::ContainerManifest
            | Self::RuntimeManifest
            | Self::EnvExample
            | Self::FrameworkRoute
            | Self::CiWorkflow
            | Self::ToolchainManifest
            | Self::DocsLinkback
            | Self::LocalGitRemote
            | Self::LocalGitCommit
            | Self::SessionMetadata
            | Self::SessionJsonl
            | Self::SessionJson
            | Self::AgentInvocationEvent
            | Self::ToolCallEvent
            | Self::ToolResultEvent
            | Self::SkillInvocationEvent
            | Self::ConversationReference => Authority::Inferred,
            Self::DerivedSourceAttribution => Authority::Mirror,
            Self::TextMention => Authority::Inferred,
        }
    }
}

impl fmt::Display for EvidenceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EvidenceKind {
    type Err = axon_api::source::ApiError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| {
                graph_validation_error(format!("unknown graph evidence kind: {value:?}"))
            })
    }
}

#[cfg(test)]
#[path = "evidence_tests.rs"]
mod tests;
