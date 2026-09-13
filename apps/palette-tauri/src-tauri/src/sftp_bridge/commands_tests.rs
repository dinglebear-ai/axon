use super::*;

fn entry(name: &str) -> SftpEntry {
    SftpEntry {
        name: name.to_string(),
        path: name.to_string(),
        is_dir: false,
        size: 0,
        modified_unix: None,
    }
}

#[test]
fn directory_packet_admission_stops_at_the_exact_limit() {
    let mut entries = Vec::new();
    assert!(!push_entry_bounded(&mut entries, entry("a"), 2));
    assert!(!push_entry_bounded(&mut entries, entry("b"), 2));
    assert!(push_entry_bounded(&mut entries, entry("c"), 2));
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "a");
    assert_eq!(entries[1].name, "b");
}

#[test]
fn malformed_lossy_filename_is_rejected_before_it_becomes_a_path() {
    assert!(validate_sftp_filename("valid-雪.txt").is_ok());
    for invalid_bytes in [b"bad-\xff.txt".as_slice(), b"bad-\xc3\x28.txt".as_slice()] {
        let lossy = String::from_utf8_lossy(invalid_bytes);
        let error = validate_sftp_filename(&lossy).unwrap_err();
        assert_eq!(
            error,
            "SFTP directory contains a filename that is not safely representable as UTF-8"
        );
    }

    // russh-sftp decodes names lossily, so an actual U+FFFD is indistinguishable
    // from malformed wire bytes. Reject it conservatively with the same UI error.
    assert_eq!(
        validate_sftp_filename("valid-unicode-replacement-\u{FFFD}.txt"),
        Err(
            "SFTP directory contains a filename that is not safely representable as UTF-8"
                .to_string()
        )
    );
}

#[test]
fn file_packet_is_rejected_before_retained_bytes_exceed_the_cap() {
    let mut bytes = vec![1, 2, 3];
    assert!(io::append_read_packet(&mut bytes, &[4, 5], 5));
    assert!(!io::append_read_packet(&mut bytes, &[6], 5));
    assert_eq!(bytes, vec![1, 2, 3, 4, 5]);
}

#[tokio::test]
async fn file_reader_enforces_actual_bytes_and_closes_after_a_lying_stat() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    // The caller's stat may claim this file is small. The read boundary must
    // still reject the actual sixth byte and close the remote handle.
    let packets = Arc::new(Mutex::new(vec![vec![4, 5, 6], vec![1, 2, 3]]));
    let closed = Arc::new(AtomicBool::new(false));
    let read_packets = Arc::clone(&packets);
    let close_observed = Arc::clone(&closed);
    let error = io::read_packets_bounded(
        5,
        tokio::time::Instant::now() + std::time::Duration::from_secs(1),
        move |_offset, _request_len| {
            let packet = read_packets.lock().unwrap().pop();
            async move { Ok(packet) }
        },
        move || async move {
            close_observed.store(true, Ordering::SeqCst);
        },
    )
    .await
    .unwrap_err();

    assert_eq!(error, "file is too large to preview (limit 5)");
    assert!(closed.load(Ordering::SeqCst));
}
