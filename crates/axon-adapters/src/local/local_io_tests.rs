use super::*;

#[test]
fn bounded_reader_rejects_bytes_arriving_past_the_metadata_size_check() {
    let reader = std::io::Cursor::new(b"sixteen bytes!!!".to_vec());
    let error = read_bounded(reader, Path::new("growing.txt"), 5)
        .expect_err("bytes beyond the admitted limit must be rejected while reading");
    assert_eq!(error.code.to_string(), "adapter.local.file_too_large");
}
