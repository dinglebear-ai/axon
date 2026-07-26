use std::collections::{BTreeMap, BTreeSet};

use super::{Finding, ManifestFinding};

#[derive(Clone, Copy)]
pub(crate) struct ReachException {
    pub path: &'static str,
    pub rule: &'static str,
    pub owner: &'static str,
    pub expected_count: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct ManifestException {
    pub path: &'static str,
    pub dependency: &'static str,
    pub table: &'static str,
    pub owner: &'static str,
    pub expected_count: usize,
}

pub(super) fn apply_reach_exceptions(
    findings: Vec<Finding>,
    exceptions: &[ReachException],
    violations: &mut Vec<String>,
) {
    let mut actual = BTreeMap::<(&str, &str), usize>::new();
    for finding in &findings {
        *actual
            .entry((finding.path.as_str(), finding.rule.as_str()))
            .or_default() += 1;
    }
    validate_unique_reach_exceptions(exceptions, violations);

    let allowed = exceptions
        .iter()
        .map(|exception| ((exception.path, exception.rule), exception.expected_count))
        .collect::<BTreeMap<_, _>>();
    for exception in exceptions {
        validate_owner(exception.owner, exception.path, exception.rule, violations);
        let count = actual
            .get(&(exception.path, exception.rule))
            .copied()
            .unwrap_or_default();
        if count != exception.expected_count {
            violations.push(format!(
                "{} exception `{}` owned by {} expected {} occurrence(s), found {count}",
                exception.path, exception.rule, exception.owner, exception.expected_count
            ));
        }
    }
    for finding in findings {
        if !allowed.contains_key(&(finding.path.as_str(), finding.rule.as_str())) {
            violations.push(format!(
                "{}  [{}] {}",
                finding.path, finding.rule, finding.detail
            ));
        }
    }
}

pub(super) fn apply_manifest_exceptions(
    findings: Vec<ManifestFinding>,
    exceptions: &[ManifestException],
    violations: &mut Vec<String>,
) {
    let mut actual = BTreeMap::<(&str, &str, &str), usize>::new();
    for finding in &findings {
        *actual
            .entry((
                finding.path.as_str(),
                finding.dependency.as_str(),
                finding.table.as_str(),
            ))
            .or_default() += 1;
    }
    validate_unique_manifest_exceptions(exceptions, violations);

    let allowed = exceptions
        .iter()
        .map(|exception| {
            (
                (exception.path, exception.dependency, exception.table),
                exception.expected_count,
            )
        })
        .collect::<BTreeMap<_, _>>();
    for exception in exceptions {
        validate_owner(
            exception.owner,
            exception.path,
            exception.dependency,
            violations,
        );
        let count = actual
            .get(&(exception.path, exception.dependency, exception.table))
            .copied()
            .unwrap_or_default();
        if count != exception.expected_count {
            violations.push(format!(
                "{} [{}] exception `{}` owned by {} expected {} occurrence(s), found {count}",
                exception.path,
                exception.table,
                exception.dependency,
                exception.owner,
                exception.expected_count
            ));
        }
    }
    for finding in findings {
        if !allowed.contains_key(&(
            finding.path.as_str(),
            finding.dependency.as_str(),
            finding.table.as_str(),
        )) {
            violations.push(format!(
                "{} declares [{}] dependency on `{}` — transports must go through axon-services",
                finding.path, finding.table, finding.dependency
            ));
        }
    }
}

fn validate_owner(owner: &str, path: &str, rule: &str, violations: &mut Vec<String>) {
    if !owner.starts_with("axon_rust-") {
        violations.push(format!(
            "{path} exception `{rule}` has missing or invalid owner `{owner}`"
        ));
    }
}

fn validate_unique_reach_exceptions(exceptions: &[ReachException], violations: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    for exception in exceptions {
        if !seen.insert((exception.path, exception.rule)) {
            violations.push(format!(
                "{} has duplicate exception for `{}`",
                exception.path, exception.rule
            ));
        }
        if exception.expected_count == 0 {
            violations.push(format!(
                "{} exception `{}` has invalid expected count 0",
                exception.path, exception.rule
            ));
        }
    }
}

fn validate_unique_manifest_exceptions(
    exceptions: &[ManifestException],
    violations: &mut Vec<String>,
) {
    let mut seen = BTreeSet::new();
    for exception in exceptions {
        if !seen.insert((exception.path, exception.dependency, exception.table)) {
            violations.push(format!(
                "{} [{}] has duplicate exception for `{}`",
                exception.path, exception.table, exception.dependency
            ));
        }
        if exception.expected_count == 0 {
            violations.push(format!(
                "{} [{}] exception `{}` has invalid expected count 0",
                exception.path, exception.table, exception.dependency
            ));
        }
    }
}
