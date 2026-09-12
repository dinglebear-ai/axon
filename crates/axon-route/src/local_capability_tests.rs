use axon_api::SourceScope;

use crate::AdapterRegistry;

#[test]
fn local_adapter_definition_exposes_target_scopes_and_options() {
    let registry = AdapterRegistry::target_defaults();
    let local = registry.find("local").expect("local adapter exists");

    for scope in [
        SourceScope::File,
        SourceScope::Directory,
        SourceScope::Workspace,
        SourceScope::Repo,
        SourceScope::Map,
    ] {
        assert!(
            local.supported_scopes.contains(&scope),
            "missing local scope {scope:?}"
        );
    }

    for option in [
        "include_globs",
        "exclude_globs",
        "respect_gitignore",
        "follow_symlinks",
        "max_file_bytes",
        "binary_policy",
        "watch_policy",
    ] {
        assert!(
            local.allowed_option_keys.contains(&option.to_string()),
            "missing local option {option}"
        );
    }
}

#[test]
fn upload_adapter_definition_exposes_max_file_bytes_override() {
    let registry = AdapterRegistry::target_defaults();
    let upload = registry.find("upload").expect("upload adapter exists");
    assert!(
        upload
            .allowed_option_keys
            .contains(&"max_file_bytes".to_string()),
        "upload callers must be able to lower the shared staged-upload byte ceiling"
    );
}
