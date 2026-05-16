use super::local_filename_exists_case_insensitive;
use serial_test::serial;
use std::env;
use std::path::{Path, PathBuf};

struct CurrentDirGuard {
    original: PathBuf,
}

impl CurrentDirGuard {
    fn change_to(path: &Path) -> Self {
        let original = env::current_dir().expect("current dir");
        env::set_current_dir(path).expect("set current dir");
        Self { original }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        let _ = env::set_current_dir(&self.original);
    }
}

#[tokio::test]
#[serial]
async fn local_filename_exists_matches_case_insensitively() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let _guard = CurrentDirGuard::change_to(temp.path());
    tokio::fs::write(temp.path().join("README.MD"), "test")
        .await
        .expect("write file");

    assert!(local_filename_exists_case_insensitive("readme.md").await);
}
