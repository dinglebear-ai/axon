use super::*;

#[test]
fn local_file_size_limit_defaults_to_sixteen_mib() {
    let options = validate_options(&AdapterOptions::default()).expect("options");
    assert_eq!(options.max_file_bytes, DEFAULT_LOCAL_MAX_FILE_BYTES);
}

#[test]
fn explicit_local_file_size_limit_overrides_default() {
    let mut values = MetadataMap::new();
    values.insert("max_file_bytes".to_string(), 4096.into());
    let options = validate_options(&AdapterOptions { values }).expect("options");
    assert_eq!(options.max_file_bytes, 4096);
}

#[test]
fn upload_options_use_the_shared_staged_upload_limit() {
    let options = validate_upload_options(&AdapterOptions::default()).expect("options");
    assert_eq!(options.max_file_bytes, MAX_UPLOAD_BYTES);
}

#[test]
fn binary_suffix_does_not_select_content_policy() {
    for policy in ["skip", "metadata", "include"] {
        let mut values = MetadataMap::new();
        values.insert("binary_policy".into(), policy.into());
        let options = validate_options(&AdapterOptions { values }).unwrap();
        assert!(options.should_include_file(SourceScope::File, "text.png", Path::new("text.png")));
        assert!(options.should_include_file(
            SourceScope::Directory,
            "binary.txz",
            Path::new("binary.txz")
        ));
        assert!(!options.should_include_file(SourceScope::Repo, "text.png", Path::new("text.png")));
        assert!(options.should_include_file(
            SourceScope::Repo,
            "some.dir/Makefile",
            Path::new("some.dir/Makefile")
        ));
    }
}
