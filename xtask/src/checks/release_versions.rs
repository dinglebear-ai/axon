use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

type ReleaseResult<T> = std::result::Result<T, ReleaseVersionError>;

macro_rules! release_bail {
    ($($arg:tt)*) => {
        return Err($crate::checks::release_versions::ReleaseVersionError::msg(format!($($arg)*)))
    };
}

mod error;
mod files;
mod git;
mod manifest;
mod release_please;

use error::{ReleaseContext, ReleaseVersionError};
use manifest::validate_manifest;
use release_please::{
    ReleasePleaseDispatchItem, ReleasePleaseFixupItem, release_please_dispatch_items,
    run_cargo_update,
};

#[cfg(test)]
use manifest::same_version_file;

use files::{
    check_component_parity, read_version, read_workspace_package_version, write_version_file,
};
use git::{
    changed_paths_since_ref, check_gradle_version_code_increased, compare_ref_for_component,
    component_changed_since_ref, latest_tag, merge_base, tag_exists,
};

#[cfg(test)]
use files::{
    increment_gradle_version_code, read_cargo_lock_package_version, read_cargo_package_version,
    read_gradle_version_code, read_gradle_version_name, read_json_version,
    read_npm_package_lock_version,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum GateMode {
    Pr,
    Main,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum BumpLevel {
    Patch,
    Minor,
    Major,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentPlan {
    pub id: String,
    pub name: String,
    pub changed: bool,
    pub version: String,
    pub candidate_tag: String,
    pub last_tag: Option<String>,
    pub release_workflow: String,
    pub shipping_paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    schema_version: u32,
    components: Vec<Component>,
}

#[derive(Debug, Deserialize)]
struct Component {
    id: String,
    name: String,
    tag_prefix: String,
    release_please_path: String,
    release_workflow: String,
    shipping_paths: Vec<String>,
    version_source: VersionFile,
    version_files: Vec<VersionFile>,
    /// Whether release-please still opens release PRs for this component.
    /// Defaults to `true`. `cli` sets this to `false`: release-please's
    /// candidate-PR build crashes on any Cargo workspace member using
    /// `version.workspace = true` (upstream bug, still open:
    /// googleapis/release-please#2478), which made it impossible to release
    /// the root Cargo workspace package through release-please at all — see
    /// CLAUDE.md's Release Pipeline section. `cli` is bumped manually via
    /// `cargo xtask bump-version cli`; a component with this set to `false`
    /// is exempt from `check_manifest_versions`'s
    /// `.release-please-manifest.json` consistency check, since it has no
    /// entry there to be consistent with.
    #[serde(default = "default_release_please_managed")]
    release_please_managed: bool,
}

fn default_release_please_managed() -> bool {
    true
}

#[derive(Debug, Deserialize, Clone)]
struct VersionFile {
    kind: VersionKind,
    path: String,
    package: Option<String>,
    json_pointer: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum VersionKind {
    CargoPackage,
    CargoLockPackage,
    ReadmeVersionLine,
    ChangelogHeading,
    JsonVersion,
    JsonNoVersion,
    NpmPackageLock,
    GradleVersionName,
    GradleVersionCode,
}

pub fn check(
    root: &Path,
    base: Option<&str>,
    head: &str,
    mode: GateMode,
    json: bool,
) -> ReleaseResult<()> {
    let manifest = load_manifest(root)?;
    let plans = build_plan(root, &manifest, base, head, mode)?;
    let mut errors = Vec::new();
    errors.extend(release_please::check_manifest_versions(
        root,
        &manifest.components,
    )?);

    for (component, plan) in manifest.components.iter().zip(plans.iter()) {
        let parity_errors = check_component_parity(root, component, &plan.version)?;
        errors.extend(
            parity_errors
                .into_iter()
                .map(|error| format!("{}: {error}", component.id)),
        );

        if !plan.changed {
            continue;
        }

        collect_changed_component_errors(root, component, plan, base, head, mode, &mut errors)?;
    }

    if !errors.is_empty() {
        for error in &errors {
            eprintln!("release version error: {error}");
        }
        release_bail!(
            "release version check failed ({} error(s)): {}",
            errors.len(),
            errors.join("; ")
        );
    }

    print_plans(&plans, json)?;

    Ok(())
}

pub fn plan(
    root: &Path,
    base: Option<&str>,
    head: &str,
    mode: GateMode,
) -> ReleaseResult<Vec<ComponentPlan>> {
    let manifest = load_manifest(root)?;
    build_plan(root, &manifest, base, head, mode)
}

pub fn print_plans(plans: &[ComponentPlan], json: bool) -> ReleaseResult<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(plans)
                .release_context("failed to serialize release plan")?
        );
    } else {
        for plan in plans {
            println!(
                "{} changed={} version={} tag={} last_tag={} workflow={}",
                plan.id,
                plan.changed,
                plan.version,
                plan.candidate_tag,
                plan.last_tag.as_deref().unwrap_or("-"),
                plan.release_workflow
            );
        }
    }
    Ok(())
}

pub fn release_please_fixups(root: &Path, component_id: &str, version: &str) -> ReleaseResult<()> {
    let manifest = load_manifest(root)?;
    release_please::fixups(root, &manifest.components, component_id, version)
}

pub fn release_please_fixup_plan(
    root: &Path,
    files: &str,
) -> ReleaseResult<Vec<ReleasePleaseFixupItem>> {
    let manifest = load_manifest(root)?;
    release_please::fixup_items(root, &manifest.components, files)
}

pub fn print_release_please_fixup_plan(
    items: &[ReleasePleaseFixupItem],
    json: bool,
) -> ReleaseResult<()> {
    release_please::print_fixup_plan(items, json)
}

pub fn release_please_dispatch_plan(
    root: &Path,
    release_outputs: &str,
) -> ReleaseResult<Vec<ReleasePleaseDispatchItem>> {
    let manifest = load_manifest(root)?;
    release_please_dispatch_items(&manifest.components, release_outputs)
}

pub fn print_release_please_dispatch_plan(
    items: &[ReleasePleaseDispatchItem],
    json: bool,
) -> ReleaseResult<()> {
    release_please::print_dispatch_plan(items, json)
}

pub fn check_local(root: &Path) -> ReleaseResult<()> {
    let manifest = load_manifest(root)?;
    let mut errors = Vec::new();

    for component in &manifest.components {
        let version = read_version(root, &component.version_source)?;
        Version::parse(&version).with_release_context(|| {
            format!("{} version is not valid semver: {version}", component.id)
        })?;
        let parity_errors = check_component_parity(root, component, &version)?;
        errors.extend(
            parity_errors
                .into_iter()
                .map(|error| format!("{}: {error}", component.id)),
        );
        if component.id == "cli"
            && let Err(error) = check_workspace_package_version(root, &version)
        {
            errors.push(format!("{}: {error}", component.id));
        }
    }

    if !errors.is_empty() {
        for error in &errors {
            eprintln!("release version error: {error}");
        }
        release_bail!(
            "local release version check failed ({} error(s)): {}",
            errors.len(),
            errors.join("; ")
        );
    }

    println!("OK: all release version files are valid in the local checkout.");
    Ok(())
}

/// Assert the root manifest's `[workspace.package] version` (inherited by every
/// extracted crate via `version.workspace = true`) equals the product version.
///
/// The release-version readers treat `[package] version` as the authoritative
/// product version, but they cannot resolve workspace inheritance, so `axon`
/// keeps an explicit `[package] version`. This guard closes the gap: it fails if
/// the two ever drift, regardless of how (manual edit, a future tool, a partial
/// bump), so a stale workspace version — and thus a wrong `CARGO_PKG_VERSION`
/// baked into every crate — cannot slip through the gate. A no-op when the root
/// manifest declares no `[workspace.package] version`.
fn check_workspace_package_version(root: &Path, product_version: &str) -> ReleaseResult<()> {
    let manifest_path = root.join("Cargo.toml");
    let content = std::fs::read_to_string(&manifest_path)
        .with_release_context(|| format!("reading {}", manifest_path.display()))?;
    if let Some(workspace_version) = read_workspace_package_version(&content)?
        && workspace_version != product_version
    {
        release_bail!(
            "[workspace.package] version ({workspace_version}) does not match the product \
             [package] version ({product_version}); they must stay equal so every crate's \
             inherited version tracks releases"
        );
    }
    Ok(())
}

pub fn check_cli_parity_only(root: &Path) -> ReleaseResult<()> {
    let manifest = load_manifest(root)?;
    let component = manifest
        .components
        .iter()
        .find(|component| component.id == "cli")
        .release_context("release manifest is missing cli component")?;
    let version = read_version(root, &component.version_source)?;
    Version::parse(&version).with_release_context(|| {
        format!("{} version is not valid semver: {version}", component.id)
    })?;
    let mut errors = check_component_parity(root, component, &version)?;
    if let Err(error) = check_workspace_package_version(root, &version) {
        errors.push(error.to_string());
    }
    if !errors.is_empty() {
        for error in &errors {
            eprintln!("version sync error: cli: {error}");
        }
        release_bail!("version sync check failed ({} error(s))", errors.len());
    }
    println!("OK: all CLI version-bearing files are in sync at {version}.");
    Ok(())
}

/// Manually bump one component's version-bearing files. The only supported
/// use today is `cli`: release-please can no longer manage it because its
/// candidate-PR build crashes on any Cargo workspace member using
/// `version.workspace = true` (upstream bug, still open:
/// googleapis/release-please#2478) — see `release/components.toml`'s comment
/// and CLAUDE.md's Release Pipeline section. `palette`/`android`/`chrome`
/// remain entirely release-please-managed and are not expected to need this.
pub fn bump_component_version(
    root: &Path,
    component_id: &str,
    level: BumpLevel,
) -> ReleaseResult<()> {
    let manifest = load_manifest(root)?;
    let component = manifest
        .components
        .iter()
        .find(|component| component.id == component_id)
        .with_release_context(|| format!("unknown release component {component_id}"))?;

    let current = read_version(root, &component.version_source)?;
    let current = Version::parse(&current).with_release_context(|| {
        format!("{} version is not valid semver: {current}", component.id)
    })?;
    let next = match level {
        BumpLevel::Patch => Version::new(current.major, current.minor, current.patch + 1),
        BumpLevel::Minor => Version::new(current.major, current.minor + 1, 0),
        BumpLevel::Major => Version::new(current.major + 1, 0, 0),
    }
    .to_string();

    for file in &component.version_files {
        if file.kind != VersionKind::CargoLockPackage {
            write_version_file(root, file, &next)?;
        }
    }
    // Cargo.lock entries are regenerated (not hand-written) after the owning
    // Cargo.toml is already bumped, mirroring the release-please fixup path's
    // `run_cargo_update`.
    for file in &component.version_files {
        if file.kind == VersionKind::CargoLockPackage {
            let package = file
                .package
                .as_deref()
                .release_context("cargo_lock_package requires package")?;
            run_cargo_update(root, package, &next)?;
        }
    }

    println!("Bumped {} {current} -> {next}", component.id);
    Ok(())
}

fn load_manifest(root: &Path) -> ReleaseResult<Manifest> {
    let path = root.join("release/components.toml");
    let content =
        std::fs::read_to_string(&path).release_context("failed to read release/components.toml")?;
    let manifest: Manifest =
        toml::from_str(&content).release_context("failed to parse release/components.toml")?;
    if manifest.schema_version != 1 {
        release_bail!(
            "unsupported release/components.toml schema_version {}",
            manifest.schema_version
        );
    }
    validate_manifest(root, &manifest)?;
    Ok(manifest)
}

fn build_plan(
    root: &Path,
    manifest: &Manifest,
    base: Option<&str>,
    head: &str,
    mode: GateMode,
) -> ReleaseResult<Vec<ComponentPlan>> {
    manifest
        .components
        .iter()
        .map(|component| {
            let version = read_version(root, &component.version_source)?;
            let candidate_version = Version::parse(&version).with_release_context(|| {
                format!("{} version is not valid semver: {version}", component.id)
            })?;
            let candidate_tag = format!("{}{}", component.tag_prefix, version);
            let last_tag = latest_tag(root, &component.tag_prefix)?;
            let changed = match mode {
                GateMode::Pr => {
                    let base = base.unwrap_or("origin/main");
                    let compare_ref = merge_base(root, base, head)?;
                    component_changed_since_ref(root, component, &compare_ref, head)?
                }
                GateMode::Main => match last_tag.as_deref() {
                    Some(tag) => {
                        let component_changed =
                            component_changed_since_ref(root, component, tag, head)?;
                        if !component_changed {
                            false
                        } else {
                            let latest_version = version_from_tag(component, tag)
                                .with_release_context(|| {
                                    format!(
                                        "{} latest tag has invalid version: {tag}",
                                        component.id
                                    )
                                })?;
                            candidate_version > latest_version && !tag_exists(root, &candidate_tag)?
                        }
                    }
                    None => true,
                },
            };
            Ok(ComponentPlan {
                id: component.id.clone(),
                name: component.name.clone(),
                changed,
                version,
                candidate_tag,
                last_tag,
                release_workflow: component.release_workflow.clone(),
                shipping_paths: component.shipping_paths.clone(),
            })
        })
        .collect()
}

fn collect_changed_component_errors(
    root: &Path,
    component: &Component,
    plan: &ComponentPlan,
    base: Option<&str>,
    head: &str,
    mode: GateMode,
    errors: &mut Vec<String>,
) -> ReleaseResult<()> {
    let candidate = Version::parse(&plan.version).with_release_context(|| {
        format!(
            "{} version is not valid semver: {}",
            component.id, plan.version
        )
    })?;

    let latest = latest_version_from_plan(component, plan)?;
    let existing_candidate_tag = tag_exists(root, &plan.candidate_tag)?;
    let release_fixup_only = release_fixup_only_pr_change(
        root,
        component,
        &candidate,
        latest.as_ref(),
        base,
        head,
        mode,
    )?;
    if !release_fixup_only {
        if let Some(latest) = latest
            && candidate <= latest
        {
            errors.push(format!(
                "{} code changed but version {} is not greater than latest {} tag version {}. Let release-please bump {} before merging.",
                component.id,
                plan.version,
                component.tag_prefix,
                latest,
                bump_hint(component)
            ));
        }

        if existing_candidate_tag {
            errors.push(format!(
                "{} code changed but tag {} already exists. Let release-please bump {} before merging.",
                component.id,
                plan.candidate_tag,
                bump_hint(component)
            ));
        }
    }

    if component_has_kind(component, VersionKind::GradleVersionCode)
        && let Some(compare_ref) = compare_ref_for_component(root, component, base, head, mode)?
        && let Err(error) = check_gradle_version_code_increased(root, component, &compare_ref)
    {
        errors.push(format!("{}: {error}", component.id));
    }

    Ok(())
}

fn release_fixup_only_pr_change(
    root: &Path,
    component: &Component,
    candidate: &Version,
    latest: Option<&Version>,
    base: Option<&str>,
    head: &str,
    mode: GateMode,
) -> ReleaseResult<bool> {
    if mode != GateMode::Pr || !component.release_please_managed {
        return Ok(false);
    }
    if latest != Some(candidate) {
        return Ok(false);
    }
    let Some(compare_ref) = compare_ref_for_component(root, component, base, head, mode)? else {
        return Ok(false);
    };
    let changed = changed_paths_since_ref(root, &compare_ref, head, &component.shipping_paths)?;
    if changed.is_empty() {
        return Ok(false);
    }
    let allowed = component
        .version_files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<BTreeSet<_>>();
    Ok(changed.iter().all(|path| allowed.contains(path.as_str())))
}

fn latest_version_from_plan(
    component: &Component,
    plan: &ComponentPlan,
) -> ReleaseResult<Option<Version>> {
    plan.last_tag
        .as_deref()
        .map(|tag| version_from_tag(component, tag))
        .transpose()
}

fn version_from_tag(component: &Component, tag: &str) -> ReleaseResult<Version> {
    let version = tag
        .strip_prefix(&component.tag_prefix)
        .with_release_context(|| format!("{} latest tag has wrong prefix: {tag}", component.id))?;
    Version::parse(version).with_release_context(|| {
        format!(
            "{} latest tag has invalid semver suffix: {tag}",
            component.id
        )
    })
}

fn bump_hint(component: &Component) -> String {
    match component.id.as_str() {
        "android" => "apps/android/app/build.gradle.kts versionName and versionCode".to_owned(),
        "chrome" => "apps/chrome-extension/manifest.json".to_owned(),
        _ => format!("the {} version files", component.id),
    }
}

fn component_has_kind(component: &Component, kind: VersionKind) -> bool {
    component.version_files.iter().any(|file| file.kind == kind)
}

#[cfg(test)]
#[path = "release_versions_tests.rs"]
mod tests;
