use super::*;
use axon_api::source::ArtifactId;

fn raw(bytes: &[u8]) -> ContentRef {
    ContentRef::InlineBytes {
        bytes_base64: STANDARD.encode(bytes),
        mime_type: "text/plain\r\nsecret-body".into(),
    }
}

fn utf16(text: &str, le: bool) -> Vec<u8> {
    let mut bytes = if le {
        vec![0xff, 0xfe]
    } else {
        vec![0xfe, 0xff]
    };
    for unit in text.encode_utf16() {
        bytes.extend(if le {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        });
    }
    bytes
}

#[test]
fn supported_text_decodes_strictly_and_preserves_text() {
    for bytes in [
        b"hello \xe2\x98\x83".to_vec(),
        b"\xef\xbb\xbfhello \xe2\x98\x83".to_vec(),
        utf16("hello ☃", true),
        utf16("hello ☃", false),
    ] {
        assert_eq!(
            classify_content(&raw(&bytes), 64).unwrap(),
            ContentDisposition::Text("hello ☃".into())
        );
    }
    assert_eq!(
        classify_content(&raw(b"truncated \xe2\x98"), 64).unwrap(),
        skipped(ContentSkipReason::UnsupportedEncoding)
    );
    assert_eq!(
        classify_content(&raw(b"caf\xe9"), 64).unwrap(),
        skipped(ContentSkipReason::UnsupportedEncoding)
    );
}

#[test]
fn assets_are_skipped_without_trusting_mime_or_extensions() {
    for bytes in [
        b"\x89PNG\r\n\x1a\n".as_slice(),
        b"\0\0\x01\0",
        b"wOFF",
        b"wOF2",
        b"OTTO",
        b"\0\x01\0\0",
        b"ttcf",
        b"\xfd7zXZ\0",
        b"\x1f\x8b",
        b"ID3",
        b"\xff\xfb",
        b"OggS",
        b"%PDF-1.7 all ASCII",
        b"hello\0world",
        b"\x01\x02\x03",
    ] {
        assert_eq!(
            classify_content(&raw(bytes), 128).unwrap(),
            skipped(ContentSkipReason::UnsupportedBinary),
            "{bytes:?}"
        );
    }
    assert_eq!(
        classify_content(&raw(b"plain text"), 128).unwrap(),
        ContentDisposition::Text("plain text".into())
    );
}

#[test]
fn utf16_is_validated_before_nul_and_control_detection() {
    for le in [true, false] {
        assert_eq!(
            classify_content(&raw(&utf16("a\tb\nc\r\u{c}😀", le)), 64).unwrap(),
            ContentDisposition::Text("a\tb\nc\r\u{c}😀".into())
        );
        assert_eq!(
            classify_content(&raw(&utf16("a\0b", le)), 64).unwrap(),
            skipped(ContentSkipReason::UnsupportedBinary)
        );
    }
    for malformed in [
        &[0xff, 0xfe, 0x41][..],
        &[0xff, 0xfe, 0x00, 0xd8],
        &[0xfe, 0xff, 0xdc, 0x00],
    ] {
        assert_eq!(
            classify_content(&raw(malformed), 64).unwrap(),
            skipped(ContentSkipReason::UnsupportedEncoding)
        );
    }
}

#[test]
fn empty_and_unresolved_references_have_explicit_reasons() {
    for bytes in [&b""[..], &[0xef, 0xbb, 0xbf], &[0xff, 0xfe], &[0xfe, 0xff]] {
        assert_eq!(
            classify_content(&raw(bytes), 64).unwrap(),
            skipped(ContentSkipReason::EmptyContent)
        );
    }
    for reference in [
        ContentRef::Artifact {
            artifact_id: ArtifactId("secret".into()),
        },
        ContentRef::External {
            uri: "https://user:password@example.test/body".into(),
            integrity: None,
        },
    ] {
        assert_eq!(
            classify_content(&reference, 0).unwrap(),
            skipped(ContentSkipReason::UnresolvedContentReference)
        );
    }
}

#[test]
fn strict_base64_contract_errors_are_fixed_and_content_free() {
    for encoded in ["a", "aaaaa", "====", "a===", "YQ=A", "YR==", "$$$$"] {
        let content = ContentRef::InlineBytes {
            bytes_base64: encoded.into(),
            mime_type: "secret".into(),
        };
        let error = classify_content(&content, 64).unwrap_err();
        assert_eq!(error, ContentPolicyError::MalformedBase64);
        assert_eq!(error.to_string(), "malformed base64 content");
    }
}

#[test]
fn byte_bounds_are_exact_including_base64_padding() {
    for text in ["a", "ab", "abc", "☃"] {
        assert_eq!(
            classify_content(&raw(text.as_bytes()), text.len()).unwrap(),
            ContentDisposition::Text(text.into())
        );
        assert_eq!(
            classify_content(&raw(text.as_bytes()), text.len() - 1).unwrap(),
            skipped(ContentSkipReason::SizeLimitExceeded)
        );
    }
    assert_eq!(
        classify_content(&raw(b"a"), 0).unwrap(),
        skipped(ContentSkipReason::SizeLimitExceeded)
    );
    assert_eq!(
        classify_content(&raw(b""), 0).unwrap(),
        skipped(ContentSkipReason::EmptyContent)
    );
    assert_eq!(
        classify_content(&raw(b"a"), usize::MAX).unwrap(),
        ContentDisposition::Text("a".into())
    );
}

#[test]
fn utf16_expansion_is_bounded_before_allocating_text() {
    let bytes = utf16("界界界", true); // 8 raw bytes become 9 UTF-8 bytes.
    assert_eq!(
        classify_content(&raw(&bytes), 8).unwrap(),
        skipped(ContentSkipReason::SizeLimitExceeded)
    );
    assert_eq!(
        classify_content(&raw(&bytes), 9).unwrap(),
        ContentDisposition::Text("界界界".into())
    );
}

#[test]
fn authored_inline_text_is_preserved_but_bounded() {
    let content = ContentRef::InlineText {
        text: "%PDF- is a documented file signature".into(),
    };
    assert_eq!(
        classify_content(&content, 128).unwrap(),
        ContentDisposition::Text("%PDF- is a documented file signature".into())
    );
    assert_eq!(
        classify_content(&content, 1).unwrap(),
        skipped(ContentSkipReason::SizeLimitExceeded)
    );
}

#[test]
fn serialized_skip_reasons_match_stable_codes() {
    for reason in [
        ContentSkipReason::UnsupportedBinary,
        ContentSkipReason::UnsupportedEncoding,
        ContentSkipReason::EmptyContent,
        ContentSkipReason::UnresolvedContentReference,
        ContentSkipReason::SizeLimitExceeded,
    ] {
        assert_eq!(serde_json::to_value(reason).unwrap(), reason.as_str());
    }
}

#[test]
fn oversized_encoded_body_is_rejected_before_alphabet_validation() {
    let content = ContentRef::InlineBytes {
        bytes_base64: "$$$$".into(),
        mime_type: "ignored".into(),
    };
    assert_eq!(
        classify_content(&content, 0).unwrap(),
        skipped(ContentSkipReason::SizeLimitExceeded)
    );
    assert_eq!(
        classify_content(&content, 3).unwrap_err(),
        ContentPolicyError::MalformedBase64
    );
}
