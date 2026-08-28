use std::fs;

use super::*;

#[test]
fn spool_is_private_ordered_and_deduplicated() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("generation.jsonl");
    let mut spool = GenerationSpool::create(&path).unwrap();
    assert!(spool.append("a", &vec![1_u8, 2]).unwrap());
    assert!(!spool.append("a", &vec![9_u8]).unwrap());
    assert!(spool.append("b", &vec![3_u8]).unwrap());
    assert_eq!(
        spool.replay::<Vec<u8>>().unwrap(),
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
