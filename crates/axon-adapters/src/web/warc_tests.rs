use super::*;

fn item(
    uri: &str,
    status: Option<u64>,
    content_kind: ContentKind,
    text: &str,
) -> AcquiredSourceItem {
    let manifest_item = ManifestItem {
        source_id: SourceId::from("src_warc_test"),
        source_item_key: SourceItemKey::from("docs/intro"),
        canonical_uri: uri.to_string(),
        item_kind: ItemKind::WebPage,
        content_kind: Some(content_kind),
        display_path: Some("docs/intro".to_string()),
        parent_key: None,
        size_bytes: None,
        content_hash: None,
        mtime: None,
        version: None,
        fetch_plan: None,
        metadata: MetadataMap::new(),
        graph_hints: Vec::new(),
    };
    let mut metadata = MetadataMap::new();
    if let Some(status) = status {
        metadata.insert("web_status".to_string(), serde_json::json!(status));
    }
    AcquiredSourceItem {
        manifest_item,
        fetch_status: LifecycleStatus::Completed,
        content_ref: ContentRef::InlineText {
            text: text.to_string(),
        },
        raw_artifact_id: None,
        headers: RedactedHeaders {
            headers: Vec::new(),
        },
        fetched_at: Timestamp::from(chrono::Utc::now()),
        metadata,
    }
}

#[test]
fn warcinfo_record_is_spec_shaped() {
    let record = warcinfo_record();
    let content = String::from_utf8_lossy(&record);
    assert!(content.starts_with("WARC/1.1\r\n"));
    assert!(content.contains("WARC-Type: warcinfo\r\n"));
    assert!(content.contains("software: axon/"));
    assert!(content.contains("Content-Type: application/warc-fields\r\n"));
    assert!(record.ends_with(b"\r\n\r\n"));
}

#[test]
fn response_record_is_spec_shaped() {
    let acquired = item(
        "https://example.com/docs/intro",
        Some(200),
        ContentKind::Html,
        "<p>hello</p>",
    );
    let record = response_record(&acquired);
    let content = String::from_utf8_lossy(&record);
    assert!(content.starts_with("WARC/1.1\r\n"));
    assert!(content.contains("WARC-Type: response\r\n"));
    assert!(content.contains("WARC-Target-URI: https://example.com/docs/intro\r\n"));
    assert!(content.contains("Content-Type: application/http; msgtype=response\r\n"));
    assert!(content.contains("HTTP/1.1 200 OK\r\n"));
    assert!(content.contains("Content-Type: text/html\r\n"));
    assert!(content.contains("<p>hello</p>"));
    assert!(record.ends_with(b"\r\n\r\n"));
}

#[test]
fn response_record_defaults_status_to_200_when_missing() {
    let acquired = item(
        "https://example.com/rendered",
        None,
        ContentKind::Markdown,
        "# hi",
    );
    let record = response_record(&acquired);
    let content = String::from_utf8_lossy(&record);
    assert!(content.contains("HTTP/1.1 200 OK\r\n"));
}

#[test]
fn response_record_decodes_inline_bytes() {
    use base64::Engine as _;
    let mut manifest_item_metadata = MetadataMap::new();
    manifest_item_metadata.insert("web_status".to_string(), serde_json::json!(200));
    let acquired = AcquiredSourceItem {
        manifest_item: ManifestItem {
            source_id: SourceId::from("src_warc_test"),
            source_item_key: SourceItemKey::from("bin/blob"),
            canonical_uri: "https://example.com/blob".to_string(),
            item_kind: ItemKind::WebPage,
            content_kind: Some(ContentKind::BinaryMetadata),
            display_path: None,
            parent_key: None,
            size_bytes: None,
            content_hash: None,
            mtime: None,
            version: None,
            fetch_plan: None,
            metadata: MetadataMap::new(),
            graph_hints: Vec::new(),
        },
        fetch_status: LifecycleStatus::Completed,
        content_ref: ContentRef::InlineBytes {
            bytes_base64: base64::engine::general_purpose::STANDARD.encode(b"raw-bytes"),
            mime_type: "application/octet-stream".to_string(),
        },
        raw_artifact_id: None,
        headers: RedactedHeaders {
            headers: Vec::new(),
        },
        fetched_at: Timestamp::from(chrono::Utc::now()),
        metadata: manifest_item_metadata,
    };
    let record = response_record(&acquired);
    let content = String::from_utf8_lossy(&record);
    assert!(content.contains("raw-bytes"));
    assert!(content.contains("Content-Length: 9\r\n"));
}

#[test]
fn build_archive_produces_a_valid_multi_record_payload() {
    let item_a = item("https://example.com/a", Some(200), ContentKind::Html, "a");
    let item_b = item("https://example.com/b", Some(200), ContentKind::Html, "b");

    let archive = build_archive(&[item_a, item_b]);

    let content = String::from_utf8_lossy(&archive.bytes);
    assert_eq!(content.matches("WARC/1.1\r\n").count(), 3); // warcinfo + 2 responses
    assert!(content.contains("WARC-Type: warcinfo"));
    assert!(content.contains("https://example.com/a"));
    assert!(content.contains("https://example.com/b"));
    assert_eq!(archive.size_bytes, archive.bytes.len() as u64);
    assert!(archive.sha256.starts_with("sha256:"));
}

#[test]
fn build_archive_digest_changes_with_content() {
    let acquired = item("https://example.com/a", Some(200), ContentKind::Html, "a");
    let changed = item("https://example.com/a", Some(200), ContentKind::Html, "b");

    let first = build_archive(&[acquired]);
    let second = build_archive(&[changed]);

    assert_ne!(first.sha256, second.sha256);
}

#[test]
fn streamed_archive_matches_legacy_record_bytes_except_generated_ids_and_dates() {
    use crate::SourceArchiveWriter as _;
    let items = vec![
        item(
            "https://example.com/a",
            Some(200),
            ContentKind::Html,
            "<p>hello</p>",
        ),
        item(
            "https://example.com/b",
            Some(404),
            ContentKind::Markdown,
            "# missing",
        ),
    ];
    let legacy = build_archive(&items);
    let mut streamed = Box::new(StreamingArchive::new().unwrap());
    for item in &items {
        streamed.append(item).unwrap();
    }
    let archive = streamed.finish().unwrap();
    let bytes = std::fs::read(archive.file.path()).unwrap();
    fn normalized(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes)
            .split_inclusive('\n')
            .filter(|line| !line.starts_with("WARC-Record-ID:") && !line.starts_with("WARC-Date:"))
            .collect()
    }
    assert_eq!(normalized(&bytes), normalized(&legacy.bytes));
    assert_eq!(archive.size_bytes, bytes.len() as u64);
    assert_eq!(
        archive.content_hash,
        format!("sha256:{}", sha256_hex(&bytes))
    );
}

#[test]
fn streamed_archive_rejects_disk_admission_before_writing_beyond_limit() {
    let mut archive = StreamingArchive::new().unwrap();
    let before = archive.file.as_file().metadata().unwrap().len();
    archive.bytes = 256 * 1024 * 1024;
    assert!(archive.write_bytes(b"x").is_err());
    assert_eq!(archive.file.as_file().metadata().unwrap().len(), before);
}

#[test]
#[ignore = "explicit streamed archive memory/disk profile"]
fn streamed_archive_memory_profile() {
    use crate::SourceArchiveWriter as _;
    let records: usize = std::env::var("AXON_WARC_PROFILE_RECORDS")
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=200).contains(&records));
    let body = "x".repeat(1024 * 1024);
    let acquired = item(
        "https://example.com/profile",
        Some(200),
        ContentKind::Markdown,
        &body,
    );
    let mut writer = Box::new(StreamingArchive::new().unwrap());
    for _ in 0..records {
        writer.append(&acquired).unwrap();
    }
    let archive = writer.finish().unwrap();
    assert!(archive.size_bytes >= records as u64 * 1024 * 1024);
    assert_eq!(
        archive.file.as_file().metadata().unwrap().len(),
        archive.size_bytes
    );
    println!(
        "WARC_PROFILE records={records} archive_bytes={} body_bytes={}",
        archive.size_bytes,
        body.len()
    );
}
