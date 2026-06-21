use super::*;
use tempfile::TempDir;

fn component(prefix: &str, paths: &[&str]) -> Component {
    Component {
        id: "test".to_owned(),
        name: "Test".to_owned(),
        tag_prefix: prefix.to_owned(),
        release_workflow: "release.yml".to_owned(),
        shipping_paths: paths.iter().map(|p| p.to_string()).collect(),
        version_source: VersionFile {
            kind: VersionKind::CargoPackage,
            path: "Cargo.toml".to_owned(),
            package: Some("axon".to_owned()),
            json_pointer: None,
        },
        version_files: Vec::new(),
    }
}

#[test]
fn include_paths_emit_file_and_dir_globs() {
    let c = component("v", &["src", "Cargo.toml"]);
    assert_eq!(
        build_include_paths(&c),
        vec![
            "src".to_owned(),
            "src/**".to_owned(),
            "Cargo.toml".to_owned(),
            "Cargo.toml/**".to_owned(),
        ]
    );
}

#[test]
fn tag_pattern_is_anchored_and_escaped() {
    // Hyphens are literal outside a character class, so they are not escaped;
    // regex metacharacters (none in these prefixes) would be. The unescaped
    // form is a valid anchored regex (spike-verified against real tags).
    assert_eq!(build_tag_pattern("v"), "^v");
    assert_eq!(build_tag_pattern("palette-v"), "^palette-v");
    assert_eq!(build_tag_pattern("chrome-ext-v"), "^chrome-ext-v");
    // A metacharacter prefix would be escaped:
    assert_eq!(build_tag_pattern("a.b"), "^a\\.b");
}

#[test]
fn parse_cliff_version_tolerates_prefixes() {
    assert_eq!(parse_cliff_version("5.17.0").unwrap().to_string(), "5.17.0");
    assert_eq!(
        parse_cliff_version("v5.17.0\n").unwrap().to_string(),
        "5.17.0"
    );
    assert_eq!(
        parse_cliff_version(" palette-v5.11.0 ")
            .unwrap()
            .to_string(),
        "5.11.0"
    );
    assert!(parse_cliff_version("not-a-version").is_err());
}

#[test]
fn derive_level_picks_highest_change() {
    let v = |s: &str| Version::parse(s).unwrap();
    assert_eq!(
        derive_level(&v("5.16.5"), &v("6.0.0")),
        Some(BumpLevel::Major)
    );
    assert_eq!(
        derive_level(&v("5.16.5"), &v("5.17.0")),
        Some(BumpLevel::Minor)
    );
    assert_eq!(
        derive_level(&v("5.16.5"), &v("5.16.6")),
        Some(BumpLevel::Patch)
    );
    assert_eq!(derive_level(&v("5.16.5"), &v("5.16.5")), None);
}

#[test]
fn resolve_next_bumps_from_max_of_source_and_tag() {
    let v = |s: &str| Version::parse(s).unwrap();
    // Normal: source == tag.
    assert_eq!(
        resolve_next(&v("5.16.6"), Some(&v("5.16.6")), BumpLevel::Minor).to_string(),
        "5.17.0"
    );
    // Stale worktree: source (5.16.5) lags tag (5.16.6) -> bump from the tag,
    // so a patch yields 5.16.7 (not a collision at 5.16.6).
    assert_eq!(
        resolve_next(&v("5.16.5"), Some(&v("5.16.6")), BumpLevel::Patch).to_string(),
        "5.16.7"
    );
    // Source ahead of tag (manual pre-bump): bump from the source.
    assert_eq!(
        resolve_next(&v("5.17.0"), Some(&v("5.16.6")), BumpLevel::Patch).to_string(),
        "5.17.1"
    );
    // No tag: bump from the source.
    assert_eq!(
        resolve_next(&v("0.2.1"), None, BumpLevel::Major).to_string(),
        "1.0.0"
    );
}

#[test]
fn next_version_uses_git_cliff_magnitude_from_max_baseline() {
    let v = |s: &str| Version::parse(s).unwrap();
    // git-cliff bumped tag 5.16.6 -> 5.17.0 (minor); source 5.16.5 lags the tag.
    let next =
        next_version_from_outputs(&v("5.16.5"), Some(&v("5.16.6")), "v5.17.0", "cli").unwrap();
    assert_eq!(next.to_string(), "5.17.0");
}

#[test]
fn next_version_patch_does_not_collide_with_tag_on_stale_worktree() {
    let v = |s: &str| Version::parse(s).unwrap();
    // git-cliff patch-bumped tag 5.16.6 -> 5.16.7; source 5.16.5 lags.
    // Applying to max(source, tag) gives 5.16.7, not a 5.16.6 collision.
    let next =
        next_version_from_outputs(&v("5.16.5"), Some(&v("5.16.6")), "v5.16.7", "cli").unwrap();
    assert_eq!(next.to_string(), "5.16.7");
}

#[test]
fn next_version_tolerates_custom_prefix_output() {
    let v = |s: &str| Version::parse(s).unwrap();
    let next = next_version_from_outputs(
        &v("5.10.5"),
        Some(&v("5.10.5")),
        "palette-v5.11.0",
        "palette",
    )
    .unwrap();
    assert_eq!(next.to_string(), "5.11.0");
}

#[test]
fn next_version_errors_when_no_releasable_commits() {
    let v = |s: &str| Version::parse(s).unwrap();
    // git-cliff echoes the latest tag unchanged ("nothing to bump").
    let err = next_version_from_outputs(&v("5.16.6"), Some(&v("5.16.6")), "v5.16.6", "cli");
    assert!(err.is_err());
}

// ---------------------------------------------------------------------------
// Integration: exercises the real `git-cliff` binary. Skips when git-cliff is
// not on PATH so CI (which has no git-cliff) stays green.
// ---------------------------------------------------------------------------

fn git_cliff_available() -> bool {
    Command::new("git-cliff")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}

#[test]
fn next_version_end_to_end_when_git_cliff_present() {
    if !git_cliff_available() {
        eprintln!("skipping next_version_end_to_end: git-cliff not on PATH");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(
        root.join("cliff.toml"),
        "[git]\nconventional_commits = true\nfilter_unconventional = true\n\
         [bump]\nfeatures_always_bump_minor = true\nbreaking_always_bump_major = true\n",
    )
    .unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@t"]);
    git(root, &["config", "user.name", "t"]);
    std::fs::write(root.join("src/a.rs"), "// a").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "chore: init"]);
    git(root, &["tag", "v1.2.3"]);
    std::fs::write(root.join("src/b.rs"), "// b").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "feat: add b"]);

    let c = component("v", &["src", "Cargo.toml"]);
    let current = Version::parse("1.2.3").unwrap();
    let latest = Version::parse("1.2.3").unwrap();
    let next = next_version(root, &c, &current, Some(&latest)).unwrap();
    assert_eq!(next.to_string(), "1.3.0", "a feat must bump the minor");
}
