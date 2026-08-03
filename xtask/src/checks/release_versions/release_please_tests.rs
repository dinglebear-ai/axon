use super::*;
use crate::checks::release_versions::{Component, VersionFile, VersionKind};

fn component(id: &str, path: &str, managed: bool) -> Component {
    let version_file = VersionFile {
        kind: VersionKind::JsonVersion,
        path: format!("{id}.json"),
        package: None,
        json_pointer: Some("/version".to_owned()),
    };
    Component {
        id: id.to_owned(),
        name: id.to_owned(),
        tag_prefix: format!("{id}-v"),
        release_please_path: path.to_owned(),
        release_workflow: format!("{id}-release.yml"),
        shipping_paths: vec![id.to_owned()],
        version_source: version_file.clone(),
        version_files: vec![version_file],
        release_please_managed: managed,
    }
}

#[test]
fn ownership_requires_config_and_manifest_to_match_managed_paths() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("release-please-config.json"),
        r#"{"packages":{"apps/palette":{"component":"palette","release-type":"simple","include-v-in-tag":true,"tag-separator":"-"}}}"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join(".release-please-manifest.json"),
        r#"{"apps/palette":"1.0.0"}"#,
    )
    .unwrap();

    validate_release_please_ownership(
        root.path(),
        &[
            component("cli", ".", false),
            component("palette", "apps/palette", true),
        ],
    )
    .expect("declared ownership matches both release-please files");
}

#[test]
fn ownership_rejects_unmanaged_path_in_release_please_config() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("release-please-config.json"),
        r#"{"packages":{".":{"component":"cli","release-type":"simple","include-v-in-tag":true,"tag-separator":"-"},"apps/palette":{"component":"palette","release-type":"simple","include-v-in-tag":true,"tag-separator":"-"}}}"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join(".release-please-manifest.json"),
        r#"{"apps/palette":"1.0.0"}"#,
    )
    .unwrap();

    let error = validate_release_please_ownership(
        root.path(),
        &[
            component("cli", ".", false),
            component("palette", "apps/palette", true),
        ],
    )
    .expect_err("release-please config must not own unmanaged cli");
    assert!(error.to_string().contains("release-please-config.json"));
    assert!(error.to_string().contains("unexpected [.]"));
}

#[test]
fn ownership_rejects_missing_managed_manifest_path() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("release-please-config.json"),
        r#"{"packages":{"apps/palette":{"component":"palette","release-type":"simple","include-v-in-tag":true,"tag-separator":"-"}}}"#,
    )
    .unwrap();
    std::fs::write(root.path().join(".release-please-manifest.json"), "{}").unwrap();

    let error = validate_release_please_ownership(
        root.path(),
        &[component("palette", "apps/palette", true)],
    )
    .expect_err("managed palette path must be present in release-please manifest");
    assert!(error.to_string().contains(".release-please-manifest.json"));
    assert!(error.to_string().contains("missing [apps/palette]"));
}

#[test]
fn ownership_rejects_two_components_claiming_one_release_path() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("release-please-config.json"),
        r#"{"packages":{"apps/shared":{"component":"palette","release-type":"simple","include-v-in-tag":true,"tag-separator":"-"}}}"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join(".release-please-manifest.json"),
        r#"{"apps/shared":"1.0.0"}"#,
    )
    .unwrap();

    let error = validate_release_please_ownership(
        root.path(),
        &[
            component("palette", "apps/shared", true),
            component("android", "apps/shared", true),
        ],
    )
    .expect_err("a release-please path must have exactly one component owner");
    assert!(error.to_string().contains("duplicate release_please_path"));
    assert!(error.to_string().contains("apps/shared"));
}

#[test]
fn ownership_rejects_release_please_tag_configuration_drift() {
    let cases = [
        (
            "component",
            r#"{"packages":{"apps/palette":{"component":"desktop","include-v-in-tag":true,"tag-separator":"-"}}}"#,
            "desktop-v",
        ),
        (
            "include-v-in-tag",
            r#"{"packages":{"apps/palette":{"component":"palette","include-v-in-tag":false,"tag-separator":"-"}}}"#,
            "palette-",
        ),
        (
            "tag-separator",
            r#"{"packages":{"apps/palette":{"component":"palette","include-v-in-tag":true,"tag-separator":"_"}}}"#,
            "palette_v",
        ),
        (
            "include-component-in-tag",
            r#"{"packages":{"apps/palette":{"component":"palette","include-v-in-tag":true,"include-component-in-tag":false,"tag-separator":"-"}}}"#,
            "v",
        ),
    ];

    for (field, config, derived_prefix) in cases {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("release-please-config.json"), config).unwrap();
        std::fs::write(
            root.path().join(".release-please-manifest.json"),
            r#"{"apps/palette":"1.0.0"}"#,
        )
        .unwrap();

        let error = validate_release_please_ownership(
            root.path(),
            &[component("palette", "apps/palette", true)],
        )
        .expect_err("release-please tag settings must derive the component tag prefix");
        let message = error.to_string();
        assert!(message.contains("apps/palette"), "{field}: {message}");
        assert!(message.contains(derived_prefix), "{field}: {message}");
        assert!(message.contains("palette-v"), "{field}: {message}");
    }
}

#[test]
fn dispatch_rejects_release_output_for_unmanaged_component() {
    let components = [
        component("cli", ".", false),
        component("palette", "apps/palette", true),
    ];
    let outputs = r#"{
        "paths_released": "[\".\", \"apps/palette\"]",
        "cli_tag": "v1.0.0",
        "palette_tag": "palette-v1.0.0"
    }"#;

    let error = release_please_dispatch_items(Path::new("."), &components, outputs)
        .expect_err("release-please must not dispatch an unmanaged component");
    assert!(error.to_string().contains("unmanaged release path ."));
}

#[test]
fn dispatch_rejects_unknown_release_output_path() {
    let components = [component("palette", "apps/palette", true)];
    let outputs = r#"{
        "paths_released": "[\"apps/unknown\"]",
        "palette_tag": "palette-v1.0.0"
    }"#;

    let error = release_please_dispatch_items(Path::new("."), &components, outputs)
        .expect_err("unknown release-please output paths must fail closed");
    assert!(
        error
            .to_string()
            .contains("unknown release path apps/unknown")
    );
}

#[test]
fn fixups_reject_unmanaged_component() {
    let root = tempfile::tempdir().unwrap();
    let components = [component("cli", ".", false)];

    let error = fixups(root.path(), &components, "cli", "1.0.1")
        .expect_err("release-please fixups must not write unmanaged components");
    assert!(error.to_string().contains("unmanaged component cli"));
}
