//! The release command surface, shared by the `xtask-release` binary and the
//! `xtask` binary that flattens it.

use std::path::Path;

use crate::{BumpLevel, GateMode, ReleaseResult};

#[derive(Debug, clap::Subcommand)]
pub enum ReleaseCommand {
    /// Verify all releasable components have valid versions and changed shipping paths have bumps.
    CheckReleaseVersions {
        #[arg(long)]
        base: Option<String>,
        #[arg(long, default_value = "HEAD")]
        head: String,
        #[arg(long, value_enum, default_value = "pr")]
        mode: GateMode,
        #[arg(long)]
        json: bool,
    },
    /// Print the release plan consumed by GitHub Actions.
    ReleasePlan {
        #[arg(long)]
        base: Option<String>,
        #[arg(long, default_value = "HEAD")]
        head: String,
        #[arg(long, value_enum, default_value = "pr")]
        mode: GateMode,
        #[arg(long)]
        json: bool,
    },
    /// Manually bump one component's version-bearing files. Only `cli` is
    /// expected to need this — see the doc comment on
    /// `crate::bump_component_version`.
    BumpVersion {
        #[arg(long, default_value = "cli")]
        component: String,
        #[arg(value_enum)]
        level: BumpLevel,
    },
    /// Apply release-please postprocessing for files it cannot update directly.
    ReleasePleaseFixups {
        #[arg(long)]
        component: String,
        #[arg(long)]
        version: String,
        #[arg(long, default_value = "origin/main")]
        base: String,
    },
    /// Print release-please postprocessing needed for a release PR branch diff.
    ReleasePleaseFixupPlan {
        #[arg(long)]
        base: String,
        #[arg(long, default_value = "HEAD")]
        head: String,
        #[arg(long)]
        json: bool,
    },
    /// Print the artifact workflow dispatch plan from release-please outputs.
    ReleasePleaseDispatchPlan {
        #[arg(long)]
        release_outputs: String,
        #[arg(long)]
        json: bool,
    },
}

/// Execute one release command against `root`.
pub fn run(root: &Path, command: ReleaseCommand) -> ReleaseResult<()> {
    match command {
        ReleaseCommand::CheckReleaseVersions {
            base,
            head,
            mode,
            json,
        } => crate::check(root, base.as_deref(), &head, mode, json),
        ReleaseCommand::ReleasePlan {
            base,
            head,
            mode,
            json,
        } => {
            let plans = crate::plan(root, base.as_deref(), &head, mode)?;
            crate::print_plans(&plans, json)
        }
        ReleaseCommand::BumpVersion { component, level } => {
            crate::bump_component_version(root, &component, level)
        }
        ReleaseCommand::ReleasePleaseFixups {
            component,
            version,
            base,
        } => crate::release_please_fixups(root, &component, &version, Some(&base)),
        ReleaseCommand::ReleasePleaseFixupPlan { base, head, json } => {
            let items = crate::release_please_fixup_plan(root, &base, &head)?;
            crate::print_release_please_fixup_plan(&items, json)
        }
        ReleaseCommand::ReleasePleaseDispatchPlan {
            release_outputs,
            json,
        } => {
            let items = crate::release_please_dispatch_plan(root, &release_outputs)?;
            crate::print_release_please_dispatch_plan(&items, json)
        }
    }
}

#[cfg(test)]
#[path = "command_tests.rs"]
mod tests;
