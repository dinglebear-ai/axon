//! Structured skills.sh catalog acquisition inside the registry source family.
//!
//! skills.sh is a pointer/evidence catalog, not the canonical source or license
//! authority. This module never fetches the detail endpoint's file contents.

mod fetch;
mod map;

use std::path::{Path, PathBuf};

use axon_api::source::{ApiError, SourcePlan, Timestamp};
use serde::{Deserialize, Serialize};

use crate::adapter::Result;

pub(crate) use fetch::fetch_dump_to_temporary_file;
pub(crate) use map::{acquire, artifact_candidates, discover, normalize};

pub(crate) const CATALOG_URI_PREFIX: &str = "catalog://skills.sh/";
pub(crate) const DUMP_OPTION: &str = "skills_sh_dump_path";
pub(crate) const VIEW_OPTION: &str = "view";
pub(crate) const QUERY_OPTION: &str = "query";
pub(crate) const OWNER_OPTION: &str = "owner";
pub(crate) const PAGE_OPTION: &str = "page";
pub(crate) const PER_PAGE_OPTION: &str = "per_page";
pub(crate) const MAX_PAGES_OPTION: &str = "max_pages";

const DEFAULT_TOTAL_LIMIT: u64 = 100;
const HARD_TOTAL_LIMIT: u64 = 1_000;
const DEFAULT_PAGE_SIZE: u32 = 100;
const HARD_PAGE_SIZE: u32 = 500;
const DEFAULT_MAX_PAGES: u32 = 1;
const HARD_MAX_PAGES: u32 = 10;
const HARD_SEARCH_LIMIT: u32 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillsShMode {
    Leaderboard,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillsShView {
    AllTime,
    Trending,
    Hot,
}

impl SkillsShView {
    pub(crate) fn as_api_value(self) -> &'static str {
        match self {
            Self::AllTime => "all-time",
            Self::Trending => "trending",
            Self::Hot => "hot",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SkillsShOptions {
    pub(crate) mode: SkillsShMode,
    pub(crate) view: SkillsShView,
    pub(crate) query: Option<String>,
    pub(crate) owner: Option<String>,
    pub(crate) start_page: u32,
    pub(crate) per_page: u32,
    pub(crate) max_pages: u32,
    pub(crate) total_limit: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SkillsShDump {
    pub(crate) provider: String,
    pub(crate) mode: String,
    pub(crate) observed_at: Timestamp,
    pub(crate) skills: Vec<SkillsShSkill>,
    #[serde(default)]
    pub(crate) pages_fetched: u32,
    #[serde(default)]
    pub(crate) total_reported: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SkillsShSkill {
    pub(crate) id: String,
    pub(crate) slug: String,
    pub(crate) name: String,
    pub(crate) source: String,
    #[serde(default)]
    pub(crate) installs: u64,
    #[serde(rename = "sourceType")]
    pub(crate) source_type: String,
    #[serde(default, rename = "installUrl")]
    pub(crate) install_url: Option<String>,
    #[serde(default)]
    pub(crate) url: Option<String>,
    #[serde(default, rename = "isDuplicate", alias = "duplicate")]
    pub(crate) is_duplicate: Option<bool>,
    #[serde(default)]
    pub(crate) hash: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SkillsShPage {
    #[serde(default)]
    pub(crate) data: Vec<SkillsShSkill>,
    #[serde(default)]
    pub(crate) pagination: Option<SkillsShPagination>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SkillsShPagination {
    pub(crate) page: u32,
    #[serde(rename = "perPage")]
    pub(crate) per_page: u32,
    pub(crate) total: u64,
    #[serde(rename = "hasMore")]
    pub(crate) has_more: bool,
}

pub(crate) fn is_plan(plan: &SourcePlan) -> bool {
    plan.route
        .source
        .canonical_uri
        .starts_with(CATALOG_URI_PREFIX)
}

pub(crate) fn options(plan: &SourcePlan) -> Result<SkillsShOptions> {
    let values = &plan.route.validated_options.values;
    let mode = mode_from_plan(plan)?;
    let view = match optional_string(values.get(VIEW_OPTION), VIEW_OPTION)?.as_deref() {
        None | Some("all-time") => SkillsShView::AllTime,
        Some("trending") => SkillsShView::Trending,
        Some("hot") => SkillsShView::Hot,
        Some(other) => {
            return Err(option_error(
                VIEW_OPTION,
                format!("unsupported skills.sh view '{other}'"),
            ));
        }
    };
    let query = optional_string(values.get(QUERY_OPTION), QUERY_OPTION)?;
    let owner = optional_string(values.get(OWNER_OPTION), OWNER_OPTION)?;
    if mode == SkillsShMode::Search && query.as_deref().is_none_or(|query| query.trim().len() < 2) {
        return Err(option_error(
            QUERY_OPTION,
            "skills.sh search requires a query of at least two characters",
        ));
    }
    let start_page = optional_u32(values.get(PAGE_OPTION), PAGE_OPTION)?.unwrap_or(0);
    let per_page = optional_u32(values.get(PER_PAGE_OPTION), PER_PAGE_OPTION)?
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, HARD_PAGE_SIZE);
    let max_pages = optional_u32(values.get(MAX_PAGES_OPTION), MAX_PAGES_OPTION)?
        .unwrap_or(DEFAULT_MAX_PAGES)
        .clamp(1, HARD_MAX_PAGES);
    let requested_limit = plan
        .limits
        .effective
        .max_items
        .unwrap_or(DEFAULT_TOTAL_LIMIT)
        .min(HARD_TOTAL_LIMIT);
    let total_limit = usize::try_from(requested_limit).unwrap_or(HARD_TOTAL_LIMIT as usize);
    let per_page = match mode {
        SkillsShMode::Leaderboard => per_page.min(requested_limit.max(1) as u32),
        SkillsShMode::Search => per_page
            .min(HARD_SEARCH_LIMIT)
            .min(requested_limit.max(1) as u32),
    };
    Ok(SkillsShOptions {
        mode,
        view,
        query,
        owner,
        start_page,
        per_page,
        max_pages,
        total_limit,
    })
}

pub(crate) fn load_dump(plan: &SourcePlan) -> Result<SkillsShDump> {
    let path = dump_path(plan)?;
    SkillsShDump::load(&path)
}

pub(crate) fn set_dump_path(plan: &mut SourcePlan, path: &Path) {
    plan.route.validated_options.values.insert(
        DUMP_OPTION.to_string(),
        serde_json::json!(path.to_string_lossy()),
    );
}

fn dump_path(plan: &SourcePlan) -> Result<PathBuf> {
    plan.route
        .validated_options
        .values
        .get(DUMP_OPTION)
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| {
            option_error(
                DUMP_OPTION,
                "skills.sh materialization is missing its dump path",
            )
        })
}

impl SkillsShDump {
    pub(crate) fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path).map_err(|error| {
            ApiError::new(
                "adapter.skills_sh.dump_unreadable",
                axon_error::ErrorStage::Discovering,
                format!("failed to read skills.sh dump: {error}"),
            )
        })?;
        let dump: Self = serde_json::from_str(&raw).map_err(|error| {
            ApiError::new(
                "adapter.skills_sh.dump_malformed",
                axon_error::ErrorStage::Discovering,
                format!("failed to parse skills.sh dump: {error}"),
            )
        })?;
        if dump.provider != "skills.sh" {
            return Err(ApiError::new(
                "adapter.skills_sh.dump_invalid",
                axon_error::ErrorStage::Discovering,
                "skills.sh dump has the wrong provider identity",
            ));
        }
        Ok(dump)
    }
}

fn mode_from_plan(plan: &SourcePlan) -> Result<SkillsShMode> {
    match plan.route.source.canonical_uri.as_str() {
        "catalog://skills.sh/leaderboard" => Ok(SkillsShMode::Leaderboard),
        "catalog://skills.sh/search" => Ok(SkillsShMode::Search),
        other => Err(ApiError::new(
            "adapter.skills_sh.route.invalid",
            axon_error::ErrorStage::Routing,
            "skills.sh route has an unsupported canonical catalog mode",
        )
        .with_context("canonical_uri", other.to_string())),
    }
}

fn optional_string(value: Option<&serde_json::Value>, key: &str) -> Result<Option<String>> {
    match value {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|value| Some(value.trim().to_string()))
            .ok_or_else(|| option_error(key, "expected a string")),
    }
}

fn optional_u32(value: Option<&serde_json::Value>, key: &str) -> Result<Option<u32>> {
    match value {
        None => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .map(Some)
            .ok_or_else(|| option_error(key, "expected a non-negative 32-bit integer")),
    }
}

fn option_error(key: &str, message: impl Into<String>) -> ApiError {
    ApiError::new(
        "adapter.skills_sh.option.invalid",
        axon_error::ErrorStage::Routing,
        message.into(),
    )
    .with_context("option", key.to_string())
}

#[cfg(test)]
#[path = "skills_sh_tests.rs"]
mod tests;
