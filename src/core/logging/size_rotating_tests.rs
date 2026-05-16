use super::*;
use std::io::Write;

#[test]
fn rotates_when_max_bytes_exceeded() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut w = SizeRotatingFile::new(tmp.path().to_path_buf(), "axon.log".to_string(), 100, 2)
        .expect("open");

    // Write 4 chunks of 80 bytes — should produce active + .1 + .2 with .3 pruned.
    for _ in 0..4 {
        w.write_all(&[b'x'; 80]).expect("write");
        w.flush().expect("flush");
    }
    drop(w);

    assert!(tmp.path().join("axon.log").exists(), "active missing");
    assert!(tmp.path().join("axon.log.1").exists(), ".1 missing");
    assert!(tmp.path().join("axon.log.2").exists(), ".2 missing");
    assert!(
        !tmp.path().join("axon.log.3").exists(),
        ".3 should have been pruned (max_files=2)"
    );
}

#[test]
fn never_rotates_when_max_bytes_is_zero() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut w = SizeRotatingFile::new(tmp.path().to_path_buf(), "axon.log".to_string(), 0, 3)
        .expect("open");
    for _ in 0..10 {
        w.write_all(&[b'x'; 1024]).expect("write");
    }
    w.flush().expect("flush");
    assert!(tmp.path().join("axon.log").exists());
    assert!(!tmp.path().join("axon.log.1").exists());
}

#[test]
fn max_files_zero_truncates_without_archive() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut w = SizeRotatingFile::new(tmp.path().to_path_buf(), "axon.log".to_string(), 50, 0)
        .expect("open");
    w.write_all(&[b'x'; 80]).expect("write");
    w.flush().expect("flush");
    // Trigger rotation on next write
    w.write_all(&[b'y'; 10]).expect("write");
    w.flush().expect("flush");
    drop(w);

    assert!(tmp.path().join("axon.log").exists());
    assert!(!tmp.path().join("axon.log.1").exists());
}

#[test]
fn appends_to_existing_file_until_rotation() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // First session: write some bytes.
    {
        let mut w = SizeRotatingFile::new(tmp.path().to_path_buf(), "axon.log".to_string(), 100, 2)
            .expect("open");
        w.write_all(&[b'a'; 50]).expect("write");
        w.flush().expect("flush");
    }
    // Second session: should pick up size from existing file and rotate
    // when total exceeds max_bytes.
    {
        let mut w = SizeRotatingFile::new(tmp.path().to_path_buf(), "axon.log".to_string(), 100, 2)
            .expect("open");
        w.write_all(&[b'b'; 80]).expect("write");
        w.flush().expect("flush");
    }
    assert!(tmp.path().join("axon.log").exists());
    assert!(
        tmp.path().join("axon.log.1").exists(),
        "rotation should have produced .1"
    );
}
