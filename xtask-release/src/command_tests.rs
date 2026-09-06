use super::*;

use clap::Parser;

/// Mirrors how `xtask` consumes this enum: flattened alongside sibling
/// commands, so `cargo xtask check-release-versions ...` keeps parsing exactly
/// as it did when these variants lived in `xtask` itself.
///
/// `xtask` cannot be compiled on every developer machine (it reaches OpenSSL
/// through `axon-services -> git2`), so this is where the flattened surface is
/// actually proven.
#[derive(Debug, Parser)]
#[command(name = "xtask")]
struct HostCli {
    #[command(subcommand)]
    command: HostCommand,
}

#[derive(Debug, clap::Subcommand)]
enum HostCommand {
    /// Stands in for xtask's own non-release commands.
    Check,
    #[command(flatten)]
    Release(ReleaseCommand),
}

fn parse(args: &[&str]) -> HostCommand {
    HostCli::parse_from(std::iter::once("xtask").chain(args.iter().copied())).command
}

#[test]
fn flattening_keeps_sibling_commands_reachable() {
    assert!(matches!(parse(&["check"]), HostCommand::Check));
}

#[test]
fn check_release_versions_keeps_its_documented_flags() {
    let HostCommand::Release(ReleaseCommand::CheckReleaseVersions {
        base,
        head,
        mode,
        json,
    }) = parse(&[
        "check-release-versions",
        "--base",
        "origin/main",
        "--head",
        "HEAD",
        "--mode",
        "pr",
    ])
    else {
        panic!("check-release-versions did not parse");
    };
    assert_eq!(base.as_deref(), Some("origin/main"));
    assert_eq!(head, "HEAD");
    assert_eq!(mode, GateMode::Pr);
    assert!(!json);
}

#[test]
fn bump_version_defaults_to_the_cli_component() {
    let HostCommand::Release(ReleaseCommand::BumpVersion { component, level }) =
        parse(&["bump-version", "patch"])
    else {
        panic!("bump-version did not parse");
    };
    assert_eq!(component, "cli");
    assert_eq!(level, BumpLevel::Patch);
}

#[test]
fn release_please_commands_keep_their_workflow_flags() {
    let HostCommand::Release(ReleaseCommand::ReleasePleaseFixupPlan { base, head, json }) =
        parse(&[
            "release-please-fixup-plan",
            "--base",
            "origin/main",
            "--head",
            "HEAD",
            "--json",
        ])
    else {
        panic!("release-please-fixup-plan did not parse");
    };
    assert_eq!(base, "origin/main");
    assert_eq!(head, "HEAD");
    assert!(json);

    let HostCommand::Release(ReleaseCommand::ReleasePleaseFixups {
        component,
        version,
        base,
    }) = parse(&[
        "release-please-fixups",
        "--component",
        "palette",
        "--version",
        "6.2.0",
    ])
    else {
        panic!("release-please-fixups did not parse");
    };
    assert_eq!(component, "palette");
    assert_eq!(version, "6.2.0");
    assert_eq!(base, "origin/main");

    let HostCommand::Release(ReleaseCommand::ReleasePleaseDispatchPlan {
        release_outputs,
        json,
    }) = parse(&[
        "release-please-dispatch-plan",
        "--release-outputs",
        "{}",
        "--json",
    ])
    else {
        panic!("release-please-dispatch-plan did not parse");
    };
    assert_eq!(release_outputs, "{}");
    assert!(json);
}

#[test]
fn release_plan_keeps_its_defaults() {
    let HostCommand::Release(ReleaseCommand::ReleasePlan {
        base,
        head,
        mode,
        json,
    }) = parse(&["release-plan"])
    else {
        panic!("release-plan did not parse");
    };
    assert_eq!(base, None);
    assert_eq!(head, "HEAD");
    assert_eq!(mode, GateMode::Pr);
    assert!(!json);
}
