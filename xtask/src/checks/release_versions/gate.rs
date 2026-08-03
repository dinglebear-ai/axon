use semver::Version;
use std::collections::BTreeSet;
use std::path::Path;

use super::error::ReleaseContext;
use super::git::{
    changed_paths_since_ref, check_gradle_version_code_increased, compare_ref_for_component,
    tag_exists,
};
use super::{Component, ComponentPlan, GateMode, ReleaseResult, VersionKind};

pub(super) fn collect_changed_component_errors(
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

    if collect_managed_pr_ownership_errors(root, component, base, head, mode, errors)? {
        return Ok(());
    }

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

fn collect_managed_pr_ownership_errors(
    root: &Path,
    component: &Component,
    base: Option<&str>,
    head: &str,
    mode: GateMode,
    errors: &mut Vec<String>,
) -> ReleaseResult<bool> {
    if mode != GateMode::Pr || !component.release_please_managed {
        return Ok(false);
    }
    let compare_ref = compare_ref_for_component(root, component, base, head, mode)?
        .release_context("PR release check is missing a comparison ref")?;
    let changed = changed_paths_since_ref(root, &compare_ref, head, &component.shipping_paths)?;
    let version_file_paths = component
        .version_files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<BTreeSet<_>>();
    let (changed_version_files, ordinary_shipping_changes): (Vec<_>, Vec<_>) = changed
        .into_iter()
        .partition(|path| version_file_paths.contains(path.as_str()));

    if changed_version_files.is_empty() {
        return Ok(true);
    }
    if !ordinary_shipping_changes.is_empty() {
        errors.push(format!(
            "{} PR mixes ordinary shipping changes with release-please-owned version files: ordinary [{}]; version files [{}]. Keep feature PRs version-free and let release-please create the version-only release PR.",
            component.id,
            ordinary_shipping_changes.join(", "),
            changed_version_files.join(", ")
        ));
        return Ok(true);
    }
    Ok(false)
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

pub(super) fn version_from_tag(component: &Component, tag: &str) -> ReleaseResult<Version> {
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
