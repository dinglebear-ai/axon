use super::*;
use serde_json::json;

/// Serialize a mapped dump and read it back through the *real* adapter reader
/// (`RegistryDump::parse`), proving the acquire output satisfies the adapter's
/// deserialize + `validate()` contract.
fn round_trip(dump: &RegistryDump) -> RegistryDump {
    let serialized = serde_json::to_string(dump).expect("serialize");
    RegistryDump::parse(&serialized).expect("adapter reader accepts the mapped dump")
}

#[test]
fn npm_maps_and_round_trips() {
    let raw = json!({
        "name": "left-pad",
        "description": "String left pad",
        "dist-tags": { "latest": "1.3.0" },
        "homepage": "https://example.com/left-pad",
        "license": "WTFPL",
        "author": { "name": "azer" },
        "keywords": ["pad", "string"],
        "readme": "# left-pad\nreadme body",
        "time": { "1.3.0": "2018-01-01T00:00:00Z", "1.2.0": "2017-01-01T00:00:00Z" },
        "versions": {
            "1.2.0": { "version": "1.2.0", "description": "old" },
            "1.3.0": { "version": "1.3.0", "description": "current" }
        }
    });
    let dump = map_npm("left-pad", &raw).expect("map npm");
    assert_eq!(dump.registry, "npm");
    assert_eq!(dump.package, "left-pad");
    assert_eq!(dump.description.as_deref(), Some("String left pad"));
    assert_eq!(dump.license.as_deref(), Some("WTFPL"));
    assert_eq!(dump.author.as_deref(), Some("azer"));
    assert_eq!(dump.keywords, vec!["pad", "string"]);
    // Latest flagged + sorted last.
    let latest = dump.latest_version().expect("latest");
    assert_eq!(latest.version, "1.3.0");
    assert!(latest.is_latest);
    // Top-level readme backfills the flagged latest version entry.
    assert_eq!(latest.readme.as_deref(), Some("# left-pad\nreadme body"));
    assert_eq!(latest.published_at.as_deref(), Some("2018-01-01T00:00:00Z"));

    let read_back = round_trip(&dump);
    assert_eq!(read_back, dump);
}

#[test]
fn npm_scoped_license_object_and_string_author() {
    let raw = json!({
        "name": "@scope/name",
        "dist-tags": { "latest": "0.1.0" },
        "license": { "type": "MIT" },
        "author": "Jane Dev <jane@example.com>",
        "versions": { "0.1.0": { "version": "0.1.0" } }
    });
    let dump = map_npm("@scope/name", &raw).expect("map npm");
    assert_eq!(dump.package, "@scope/name");
    assert_eq!(dump.license.as_deref(), Some("MIT"));
    assert_eq!(dump.author.as_deref(), Some("Jane Dev <jane@example.com>"));
    round_trip(&dump);
}

#[test]
fn pypi_maps_and_round_trips() {
    let raw = json!({
        "info": {
            "name": "requests",
            "version": "2.31.0",
            "summary": "HTTP for Humans",
            "description": "# Requests\nlong readme",
            "home_page": "https://requests.readthedocs.io",
            "license": "Apache-2.0",
            "author": "Kenneth Reitz",
            "keywords": "http,requests client"
        },
        "releases": {
            "2.30.0": [],
            "2.31.0": []
        }
    });
    let dump = map_pypi("requests", &raw).expect("map pypi");
    assert_eq!(dump.registry, "pypi");
    assert_eq!(dump.package, "requests");
    assert_eq!(dump.description.as_deref(), Some("HTTP for Humans"));
    assert_eq!(dump.license.as_deref(), Some("Apache-2.0"));
    assert_eq!(dump.author.as_deref(), Some("Kenneth Reitz"));
    // keywords split on both comma and space.
    assert_eq!(dump.keywords, vec!["http", "requests", "client"]);
    let latest = dump.latest_version().expect("latest");
    assert_eq!(latest.version, "2.31.0");
    assert!(latest.is_latest);
    // Long-form README attaches to the latest version.
    assert_eq!(latest.readme.as_deref(), Some("# Requests\nlong readme"));
    round_trip(&dump);
}

#[test]
fn pypi_without_releases_still_has_latest() {
    let raw = json!({
        "info": { "name": "tiny", "version": "0.0.1", "summary": "s" }
    });
    let dump = map_pypi("tiny", &raw).expect("map pypi");
    assert_eq!(dump.versions.len(), 1);
    assert_eq!(dump.versions[0].version, "0.0.1");
    assert!(dump.versions[0].is_latest);
    round_trip(&dump);
}

#[test]
fn crates_maps_and_round_trips() {
    let raw = json!({
        "crate": {
            "name": "serde",
            "description": "A serialization framework",
            "homepage": "https://serde.rs",
            "keywords": ["serde", "serialization"],
            "max_stable_version": "1.0.200",
            "newest_version": "1.0.200"
        },
        "versions": [
            { "num": "1.0.199", "created_at": "2024-01-01T00:00:00Z" },
            { "num": "1.0.200", "created_at": "2024-02-01T00:00:00Z", "readme": "readme text" }
        ]
    });
    let dump = map_crates("serde", &raw).expect("map crates");
    assert_eq!(dump.registry, "crates");
    assert_eq!(dump.package, "serde");
    assert_eq!(dump.keywords, vec!["serde", "serialization"]);
    let latest = dump.latest_version().expect("latest");
    assert_eq!(latest.version, "1.0.200");
    assert!(latest.is_latest);
    assert_eq!(latest.readme.as_deref(), Some("readme text"));
    round_trip(&dump);
}

#[test]
fn empty_versions_is_an_error() {
    // A registry response with no usable versions must fail mapping, not write a
    // dump the adapter would reject.
    let raw = json!({ "name": "ghost", "dist-tags": {}, "versions": {} });
    assert!(map_npm("ghost", &raw).is_err());

    let raw = json!({ "crate": { "name": "ghost" }, "versions": [] });
    assert!(map_crates("ghost", &raw).is_err());

    let raw = json!({ "info": { "name": "ghost" } });
    assert!(map_pypi("ghost", &raw).is_err());
}
