use super::*;

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}

fn write(root: &Path, path: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("generated artifact parent")).unwrap();
    fs::write(path, "generated\n").unwrap();
}

#[test]
fn drift_detection_rejects_required_output_recreated_as_untracked() {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "--quiet"]);
    git(repo.path(), &["config", "user.name", "Axon Test"]);
    git(
        repo.path(),
        &["config", "user.email", "axon-test@example.invalid"],
    );
    git(repo.path(), &["config", "commit.gpgsign", "false"]);
    git(repo.path(), &["config", "core.hooksPath", ".git/no-hooks"]);

    for path in GENERATED_ARTIFACTS {
        write(repo.path(), path);
    }
    git(repo.path(), &["add", "."]);
    git(
        repo.path(),
        &["commit", "--quiet", "-m", "seed generated outputs"],
    );

    let removed = GENERATED_ARTIFACTS[0];
    git(repo.path(), &["rm", "--quiet", removed]);
    git(
        repo.path(),
        &["commit", "--quiet", "-m", "remove generated output"],
    );
    write(repo.path(), removed);

    let drifted = generated_artifact_drift(repo.path()).unwrap();
    assert!(
        drifted.iter().any(|path| path == removed),
        "untracked regenerated output must be reported: {drifted:?}"
    );
}
