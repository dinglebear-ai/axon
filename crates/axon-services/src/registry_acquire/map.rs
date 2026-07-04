//! Pure raw-registry-JSON → [`RegistryDump`] mapping.
//!
//! No network, no I/O — just structural mapping from each registry API's JSON
//! shape (npm / PyPI / crates.io) into the
//! [`axon_adapters::registry_sources::dump::RegistryDump`] the registry adapter
//! reads. The output MUST satisfy that type's `validate()` (non-empty registry,
//! package, and at least one non-empty version string), because the acquire
//! path writes this and the adapter reads it back through `RegistryDump::load`.
//! This is the highest-risk part of the registry acquisition slice, hence it is
//! isolated and round-trip tested through the *real* adapter reader.

use anyhow::{Result, bail};
use axon_adapters::registry_sources::dump::{RegistryDump, RegistryDumpVersion};
use serde_json::Value;

/// Map an npm `registry.npmjs.org/<pkg>` document into a [`RegistryDump`].
///
/// npm returns `{ name, description, "dist-tags": {latest}, versions: {ver:
/// {...}}, homepage, license, author, keywords, readme, time: {ver: iso} }`.
pub(super) fn map_npm(package: &str, value: &Value) -> Result<RegistryDump> {
    let latest = value
        .get("dist-tags")
        .and_then(|tags| tags.get("latest"))
        .and_then(Value::as_str);
    let top_readme = str_field(value, "readme");
    let time = value.get("time");

    let mut versions = Vec::new();
    if let Some(map) = value.get("versions").and_then(Value::as_object) {
        for (ver, entry) in map {
            let published_at = time
                .and_then(|t| t.get(ver))
                .and_then(Value::as_str)
                .map(ToString::to_string);
            versions.push(RegistryDumpVersion {
                version: ver.clone(),
                readme: str_field(entry, "readme").or_else(|| top_readme.clone()),
                description: str_field(entry, "description"),
                published_at,
                is_latest: latest == Some(ver.as_str()),
            });
        }
    }
    sort_latest_last(&mut versions, latest);

    finish(
        "npm",
        package,
        RegistryDump {
            registry: "npm".to_string(),
            package: str_field(value, "name").unwrap_or_else(|| package.to_string()),
            description: str_field(value, "description"),
            homepage: str_field(value, "homepage"),
            license: license_field(value.get("license")),
            author: author_field(value.get("author")),
            keywords: str_vec(value.get("keywords")),
            versions,
        },
    )
}

/// Map a PyPI `pypi.org/pypi/<pkg>/json` document into a [`RegistryDump`].
///
/// PyPI returns `{ info: {name, version, summary, description, home_page,
/// license, author, keywords}, releases: {ver: [...]} }`. The long-form README
/// lives in `info.description`; `info.version` is the latest release.
pub(super) fn map_pypi(package: &str, value: &Value) -> Result<RegistryDump> {
    let info = value.get("info");
    let latest = info.and_then(|i| i.get("version")).and_then(Value::as_str);
    let readme = info.and_then(|i| str_field(i, "description"));

    let mut versions = Vec::new();
    if let Some(map) = value.get("releases").and_then(Value::as_object) {
        for ver in map.keys() {
            versions.push(RegistryDumpVersion {
                version: ver.clone(),
                readme: if latest == Some(ver.as_str()) {
                    readme.clone()
                } else {
                    None
                },
                description: info.and_then(|i| str_field(i, "summary")),
                published_at: None,
                is_latest: latest == Some(ver.as_str()),
            });
        }
    }
    // A package with no `releases` map still has its `info.version`; guarantee
    // at least the latest version so the dump validates.
    if versions.is_empty()
        && let Some(latest) = latest
    {
        versions.push(RegistryDumpVersion {
            version: latest.to_string(),
            readme,
            description: info.and_then(|i| str_field(i, "summary")),
            published_at: None,
            is_latest: true,
        });
    }
    sort_latest_last(&mut versions, latest);

    finish(
        "pypi",
        package,
        RegistryDump {
            registry: "pypi".to_string(),
            package: info
                .and_then(|i| str_field(i, "name"))
                .unwrap_or_else(|| package.to_string()),
            description: info.and_then(|i| str_field(i, "summary")),
            homepage: info.and_then(|i| str_field(i, "home_page")),
            license: info.and_then(|i| str_field(i, "license")),
            author: info.and_then(|i| str_field(i, "author")),
            keywords: pypi_keywords(info),
            versions,
        },
    )
}

/// Map a crates.io `/api/v1/crates/<pkg>` document into a [`RegistryDump`].
///
/// crates.io returns `{ crate: {name, description, homepage, keywords,
/// max_stable_version, newest_version}, versions: [{num, created_at, readme?,
/// license}] }`.
pub(super) fn map_crates(package: &str, value: &Value) -> Result<RegistryDump> {
    let krate = value.get("crate");
    let latest = krate
        .and_then(|c| c.get("max_stable_version").and_then(Value::as_str))
        .or_else(|| krate.and_then(|c| c.get("newest_version").and_then(Value::as_str)));

    let mut versions = Vec::new();
    if let Some(list) = value.get("versions").and_then(Value::as_array) {
        for entry in list {
            let Some(num) = str_field(entry, "num") else {
                continue;
            };
            versions.push(RegistryDumpVersion {
                readme: str_field(entry, "readme"),
                description: str_field(entry, "description"),
                published_at: str_field(entry, "created_at"),
                is_latest: latest == Some(num.as_str()),
                version: num,
            });
        }
    }
    sort_latest_last(&mut versions, latest);

    finish(
        "crates",
        package,
        RegistryDump {
            registry: "crates".to_string(),
            package: krate
                .and_then(|c| str_field(c, "name"))
                .unwrap_or_else(|| package.to_string()),
            description: krate.and_then(|c| str_field(c, "description")),
            homepage: krate.and_then(|c| str_field(c, "homepage")),
            license: None,
            author: None,
            keywords: crates_keywords(krate),
            versions,
        },
    )
}

/// Validate the mapped dump has at least one version before returning it —
/// mirrors the adapter's own `validate()` so acquisition fails with a clear
/// message rather than writing a dump the adapter would later reject.
fn finish(registry: &str, package: &str, dump: RegistryDump) -> Result<RegistryDump> {
    if dump.versions.is_empty() {
        bail!("registry '{registry}' returned no versions for package '{package}'");
    }
    Ok(dump)
}

/// Move the latest version (when identified) to the end of the list. The
/// adapter's `latest_version()` falls back to the last entry when none is
/// flagged, and keeps ordering stable/deterministic across runs.
fn sort_latest_last(versions: &mut [RegistryDumpVersion], latest: Option<&str>) {
    versions.sort_by(|a, b| a.version.cmp(&b.version));
    if let Some(latest) = latest
        && let Some(idx) = versions.iter().position(|v| v.version == latest)
    {
        let last = versions.len() - 1;
        versions.swap(idx, last);
    }
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
}

fn str_vec(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// npm's `license` may be a string or a `{ "type": "MIT" }` object.
fn license_field(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) if !text.trim().is_empty() => Some(text.trim().to_string()),
        Some(Value::Object(map)) => map
            .get("type")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToString::to_string),
        _ => None,
    }
}

/// npm's `author` may be a string or a `{ "name": "..." }` object.
fn author_field(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) if !text.trim().is_empty() => Some(text.trim().to_string()),
        Some(Value::Object(map)) => map
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToString::to_string),
        _ => None,
    }
}

/// PyPI `info.keywords` is a comma- or space-separated string, not an array.
fn pypi_keywords(info: Option<&Value>) -> Vec<String> {
    info.and_then(|i| str_field(i, "keywords"))
        .map(|raw| {
            raw.split([',', ' '])
                .map(str::trim)
                .filter(|word| !word.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn crates_keywords(krate: Option<&Value>) -> Vec<String> {
    krate
        .map(|c| str_vec(c.get("keywords")))
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "map_tests.rs"]
mod tests;
