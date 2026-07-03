//! Closed `GraphNodeKind` registry for the SourceGraph.
//!
//! The canonical node-kind list is defined in
//! `docs/pipeline-unification/sources/source-graph.md` ("Node Kinds"). This
//! enum encodes that closed registry exactly. The graph store rejects any node
//! whose `node_kind` string does not parse into one of these variants
//! ("graph store rejects unknown kinds before write" — graph-schema.md).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::graph_validation_error;

/// A closed node kind from the canonical SourceGraph registry.
///
/// Serialized as the exact registry name (e.g. `web_origin`, `repo_file`,
/// `api_operation`). Deserialization of an unknown string fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphNodeKind {
    Source,
    WebOrigin,
    DocsSite,
    WebPage,
    Repo,
    RepoBranch,
    RepoCommit,
    RepoFile,
    LocalCheckout,
    Package,
    PackageVersion,
    RegistryNamespace,
    ContainerImage,
    ContainerImageTag,
    GithubAction,
    GithubActionRef,
    Toolchain,
    ToolchainVersion,
    SystemPackage,
    TerraformProvider,
    HelmChart,
    RuntimeService,
    NetworkEndpoint,
    VolumeMount,
    EnvironmentVariable,
    SecretReference,
    ApiSurface,
    ApiOperation,
    SchemaType,
    SchemaField,
    Protocol,
    Model,
    RedditSubreddit,
    RedditThread,
    YoutubeVideo,
    YoutubePlaylist,
    YoutubeChannel,
    Feed,
    FeedEntry,
    Session,
    SessionTurn,
    Agent,
    AgentInvocation,
    Tool,
    ToolCall,
    ExternalResource,
    Skill,
    SkillInvocation,
    Memory,
    Decision,
    Issue,
    PullRequest,
    PersonOrOrg,
    DerivedSource,
    Artifact,
}

impl GraphNodeKind {
    /// Every node kind in registry order.
    pub const ALL: &'static [GraphNodeKind] = &[
        Self::Source,
        Self::WebOrigin,
        Self::DocsSite,
        Self::WebPage,
        Self::Repo,
        Self::RepoBranch,
        Self::RepoCommit,
        Self::RepoFile,
        Self::LocalCheckout,
        Self::Package,
        Self::PackageVersion,
        Self::RegistryNamespace,
        Self::ContainerImage,
        Self::ContainerImageTag,
        Self::GithubAction,
        Self::GithubActionRef,
        Self::Toolchain,
        Self::ToolchainVersion,
        Self::SystemPackage,
        Self::TerraformProvider,
        Self::HelmChart,
        Self::RuntimeService,
        Self::NetworkEndpoint,
        Self::VolumeMount,
        Self::EnvironmentVariable,
        Self::SecretReference,
        Self::ApiSurface,
        Self::ApiOperation,
        Self::SchemaType,
        Self::SchemaField,
        Self::Protocol,
        Self::Model,
        Self::RedditSubreddit,
        Self::RedditThread,
        Self::YoutubeVideo,
        Self::YoutubePlaylist,
        Self::YoutubeChannel,
        Self::Feed,
        Self::FeedEntry,
        Self::Session,
        Self::SessionTurn,
        Self::Agent,
        Self::AgentInvocation,
        Self::Tool,
        Self::ToolCall,
        Self::ExternalResource,
        Self::Skill,
        Self::SkillInvocation,
        Self::Memory,
        Self::Decision,
        Self::Issue,
        Self::PullRequest,
        Self::PersonOrOrg,
        Self::DerivedSource,
        Self::Artifact,
    ];

    /// The exact registry name (snake_case) for this node kind.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::WebOrigin => "web_origin",
            Self::DocsSite => "docs_site",
            Self::WebPage => "web_page",
            Self::Repo => "repo",
            Self::RepoBranch => "repo_branch",
            Self::RepoCommit => "repo_commit",
            Self::RepoFile => "repo_file",
            Self::LocalCheckout => "local_checkout",
            Self::Package => "package",
            Self::PackageVersion => "package_version",
            Self::RegistryNamespace => "registry_namespace",
            Self::ContainerImage => "container_image",
            Self::ContainerImageTag => "container_image_tag",
            Self::GithubAction => "github_action",
            Self::GithubActionRef => "github_action_ref",
            Self::Toolchain => "toolchain",
            Self::ToolchainVersion => "toolchain_version",
            Self::SystemPackage => "system_package",
            Self::TerraformProvider => "terraform_provider",
            Self::HelmChart => "helm_chart",
            Self::RuntimeService => "runtime_service",
            Self::NetworkEndpoint => "network_endpoint",
            Self::VolumeMount => "volume_mount",
            Self::EnvironmentVariable => "environment_variable",
            Self::SecretReference => "secret_reference",
            Self::ApiSurface => "api_surface",
            Self::ApiOperation => "api_operation",
            Self::SchemaType => "schema_type",
            Self::SchemaField => "schema_field",
            Self::Protocol => "protocol",
            Self::Model => "model",
            Self::RedditSubreddit => "reddit_subreddit",
            Self::RedditThread => "reddit_thread",
            Self::YoutubeVideo => "youtube_video",
            Self::YoutubePlaylist => "youtube_playlist",
            Self::YoutubeChannel => "youtube_channel",
            Self::Feed => "feed",
            Self::FeedEntry => "feed_entry",
            Self::Session => "session",
            Self::SessionTurn => "session_turn",
            Self::Agent => "agent",
            Self::AgentInvocation => "agent_invocation",
            Self::Tool => "tool",
            Self::ToolCall => "tool_call",
            Self::ExternalResource => "external_resource",
            Self::Skill => "skill",
            Self::SkillInvocation => "skill_invocation",
            Self::Memory => "memory",
            Self::Decision => "decision",
            Self::Issue => "issue",
            Self::PullRequest => "pull_request",
            Self::PersonOrOrg => "person_or_org",
            Self::DerivedSource => "derived_source",
            Self::Artifact => "artifact",
        }
    }
}

impl fmt::Display for GraphNodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GraphNodeKind {
    type Err = axon_api::source::ApiError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| graph_validation_error(format!("unknown graph node kind: {value:?}")))
    }
}

#[cfg(test)]
#[path = "node_tests.rs"]
mod tests;
