use super::*;
use std::io::{Seek, SeekFrom, Write};
fn file(bytes: &[u8]) -> File {
    let mut f = tempfile::tempfile().unwrap();
    f.write_all(bytes).unwrap();
    f.seek(SeekFrom::Start(0)).unwrap();
    f
}
#[test]
fn exact_bounds_zero_and_oversize_do_not_require_an_extra_probe() {
    let exact = read_file(file(b"abc"), "item", 3, 3).unwrap();
    assert_eq!(exact.bytes, Some(b"abc".to_vec()));
    assert_eq!(exact.bytes_read, 3);
    let omitted = read_file(file(b"abcd"), "item", 3, 0).unwrap();
    assert!(omitted.bytes.is_none());
    assert_eq!(omitted.bytes_read, 0);
    assert!(read_file(file(b"a"), "item", 3, 0).is_err());
    assert_eq!(read_file(file(b""), "item", 3, 0).unwrap().bytes_read, 0);
    assert!(read_file(file(b"abc"), "item", 3, 2).is_err());
    assert_eq!(
        read_file(file(b"abc"), "item", u64::MAX, u64::MAX)
            .unwrap()
            .bytes_read,
        3
    );
}

#[test]
fn growth_after_stat_at_item_and_job_limit_is_charged_omission() {
    let mut input = file(b"ab");
    let size = input.metadata().unwrap().len();
    input.seek(SeekFrom::End(0)).unwrap();
    input.write_all(b"cd").unwrap();
    input.rewind().unwrap();
    let mut observer = input.try_clone().unwrap();
    let result = read_file_after_stat(input, "item", 3, 3, size).unwrap();
    assert!(result.bytes.is_none());
    assert_eq!(result.bytes_read, 3);
    assert_eq!(
        observer.stream_position().unwrap(),
        3,
        "no extra overflow probe"
    );
}

#[test]
fn growth_beyond_job_budget_that_fits_item_limit_still_fails() {
    let mut input = file(b"ab");
    let size = input.metadata().unwrap().len();
    input.seek(SeekFrom::End(0)).unwrap();
    input.write_all(b"cd").unwrap();
    input.rewind().unwrap();
    let mut observer = input.try_clone().unwrap();
    let error = read_file_after_stat(input, "item", 5, 3, size)
        .err()
        .unwrap();
    assert_eq!(error.code, "source.acquire.byte_budget_exceeded".into());
    assert_eq!(observer.stream_position().unwrap(), 3);
}
