use std::fs;
use std::io::Write as _;

use super::*;

#[test]
fn spool_is_private_ordered_and_deduplicated() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("generation.jsonl");
    let mut spool = GenerationSpool::create(&path).unwrap();
    assert!(spool.append("a", &vec![1_u8, 2]).unwrap());
    assert!(!spool.append("a", &vec![9_u8]).unwrap());
    assert!(spool.append("b", &vec![3_u8]).unwrap());
    let mut replayed = Vec::new();
    spool
        .replay_each::<Vec<u8>>(|key, value| {
            replayed.push((key, value));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        replayed,
        vec![("a".to_string(), vec![1, 2]), ("b".to_string(), vec![3])]
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn replay_stops_on_corruption_instead_of_silently_dropping_records() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("corrupt.jsonl");
    let mut spool = GenerationSpool::create(&path).unwrap();
    assert!(spool.append("good", &vec![1_u8]).unwrap());
    spool.file.write_all(b"{not-json}\n").unwrap();
    spool.file.flush().unwrap();

    let mut replayed = Vec::new();
    let error = spool
        .replay_each::<Vec<u8>>(|key, value| {
            replayed.push((key, value));
            Ok(())
        })
        .expect_err("corrupt replay must fail closed");

    assert_eq!(replayed, vec![("good".to_string(), vec![1])]);
    assert!(error.to_string().contains("expected"));
}
