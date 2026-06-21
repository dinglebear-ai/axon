use semver::Version;
use serde::{Deserialize, Serialize};
use std::path::Path;

type ReleaseResult<T> = std::result::Result<T, ReleaseVersionError>;

macro_rules! release_bail {
    ($($arg:tt)*) => {
        return Err($crate::checks::release_versions::ReleaseVersionError::msg(format!($($arg)*)))
    };
}

mod cliff;
mod error;
mod files;
mod git;
mod manifest;

use error::{ReleaseContext, ReleaseVersionError};
use manifest::validate_manifest;

#[cfg(test)]
use manifest::same_version_file;

use files::{
    check_component_parity, ensure_changelog_heading, increment_gradle_version_code, read_version,
    replace_cargo_lock_package_version, replace_cargo_package_version, replace_gradle_version_name,
    replace_json_version, replace_npm_package_lock_version, replace_readme_version_line,
};
use git::{
    check_gradle_version_code_increased, compare_ref_for_component, component_changed_since_ref,
    latest_tag, merge_base, tag_exists,
};

#[cfg(test)]
use files::{
    read_cargo_lock_package_version, read_cargo_package_version, read_gradle_version_code,
    read_gradle_version_name, read_json_version, read_npm_package_lock_version,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum GateMode {
    Pr,
    Main,
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
    release_workflow: String,
    shipping_paths: Vec<String>,
    version_source: VersionFile,
    version_files: Vec<VersionFile>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum BumpLevel {
    Patch,
    Minor,
    Major,
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

pub fn bump(root: &Path, component_id: &str, level: BumpLevel) -> ReleaseResult<()> {
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
    };
    let next = next.to_string();

    for file in &component.version_files {
        let path = root.join(&file.path);
        let content = std::fs::read_to_string(&path)
            .with_release_context(|| format!("failed to read {}", file.path))?;
        let updated = match file.kind {
            VersionKind::CargoPackage => {
                replace_cargo_package_version(&content, file.package.as_deref(), &next)?
            }
            VersionKind::CargoLockPackage => {
                replace_cargo_lock_package_version(&content, file.package.as_deref(), &next)?
            }
            VersionKind::ReadmeVersionLine => replace_readme_version_line(&content, &next)?,
            VersionKind::ChangelogHeading => ensure_changelog_heading(&content, &next)?,
            VersionKind::JsonVersion => {
                replace_json_version(&content, file.json_pointer.as_deref(), &next)?
            }
            VersionKind::JsonNoVersion => content.clone(),
            VersionKind::NpmPackageLock => {
                replace_npm_package_lock_version(&content, file.package.as_deref(), &next)?
            }
            VersionKind::GradleVersionName => replace_gradle_version_name(&content, &next)?,
            VersionKind::GradleVersionCode => increment_gradle_version_code(&content)?,
        };
        if updated != content {
            std::fs::write(&path, updated)
                .with_release_context(|| format!("failed to write {}", file.path))?;
        }
    }

    Ok(())
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
    let errors = check_component_parity(root, component, &version)?;
    if !errors.is_empty() {
        for error in &errors {
            eprintln!("version sync error: cli: {error}");
        }
        release_bail!("version sync check failed ({} error(s))", errors.len());
    }
    println!("OK: all CLI version-bearing files are in sync at {version}.");
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
            Version::parse(&version).with_release_context(|| {
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
                    Some(tag) => component_changed_since_ref(root, component, tag, head)?,
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
    if let Some(latest) = latest
        && candidate <= latest
    {
        errors.push(format!(
            "{} code changed but version {} is not greater than latest {} tag version {}. Bump {} before merging.",
            component.id,
            plan.version,
            component.tag_prefix,
            latest,
            bump_hint(component)
        ));
    }

    if tag_exists(root, &plan.candidate_tag)? {
        errors.push(format!(
            "{} code changed but tag {} already exists. Bump {} before merging.",
            component.id,
            plan.candidate_tag,
            bump_hint(component)
        ));
    }

    if component_has_kind(component, VersionKind::GradleVersionCode)
        && let Some(compare_ref) = compare_ref_for_component(root, component, base, head, mode)?
        && let Err(error) = check_gradle_version_code_increased(root, component, &compare_ref)
    {
        errors.push(format!("{}: {error}", component.id));
    }

    Ok(())
}

fn latest_version_from_plan(
    component: &Component,
    plan: &ComponentPlan,
) -> ReleaseResult<Option<Version>> {
    plan.last_tag
        .as_deref()
        .map(|tag| {
            let version = tag
                .strip_prefix(&component.tag_prefix)
                .with_release_context(|| {
                    format!("{} latest tag has wrong prefix: {tag}", component.id)
                })?;
            Version::parse(version).with_release_context(|| {
                format!(
                    "{} latest tag has invalid semver suffix: {tag}",
                    component.id
                )
            })
        })
        .transpose()
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
