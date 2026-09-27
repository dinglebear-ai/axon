//! Bounded content classification before redaction, parsing, and chunking.
//!
//! Limits apply separately to raw decoded bytes and the resulting UTF-8 text.
//! The caller's resident-memory admission must also cover the borrowed transport
//! representation plus both allocations. Source ranges refer to decoded UTF-8,
//! not byte offsets into a UTF-16 transport body.

use axon_api::source::{ContentRef, ContentSkipReason};
use base64::{Engine as _, engine::general_purpose::STANDARD};

pub const DEFAULT_CONTENT_BYTE_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentDisposition {
    Text(String),
    Skipped(ContentSkipReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentPolicyError {
    MalformedBase64,
}

impl std::fmt::Display for ContentPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedBase64 => f.write_str("malformed base64 content"),
        }
    }
}

impl std::error::Error for ContentPolicyError {}

/// Classify without fetching references or consulting untrusted path/MIME hints.
/// Authored InlineText is already text: binary signatures apply to raw bytes only.
pub fn classify_content(
    content: &ContentRef,
    max_bytes: usize,
) -> Result<ContentDisposition, ContentPolicyError> {
    match content {
        ContentRef::InlineText { text } => {
            if text.len() > max_bytes {
                return Ok(skipped(ContentSkipReason::SizeLimitExceeded));
            }
            Ok(classify_text(text.strip_prefix('\u{feff}').unwrap_or(text)))
        }
        ContentRef::InlineBytes { bytes_base64, .. } => {
            decode_base64_content(bytes_base64, max_bytes)
        }
        ContentRef::Artifact { .. } | ContentRef::External { .. } => {
            Ok(skipped(ContentSkipReason::UnresolvedContentReference))
        }
    }
}

fn skipped(reason: ContentSkipReason) -> ContentDisposition {
    ContentDisposition::Skipped(reason)
}

fn decode_base64_content(
    encoded: &str,
    max_bytes: usize,
) -> Result<ContentDisposition, ContentPolicyError> {
    // STANDARD requires complete padded quanta. Calculate the exact destination
    // size before allocation; decode_slice then validates alphabet and padding.
    if !encoded.len().is_multiple_of(4) {
        return Err(ContentPolicyError::MalformedBase64);
    }
    let padding = encoded
        .as_bytes()
        .iter()
        .rev()
        .take_while(|&&b| b == b'=')
        .count();
    if padding > 2 {
        return Err(ContentPolicyError::MalformedBase64);
    }
    let Some(decoded_len) = (encoded.len() / 4)
        .checked_mul(3)
        .and_then(|n| n.checked_sub(padding))
    else {
        return Err(ContentPolicyError::MalformedBase64);
    };
    // Resource admission takes precedence over validating an oversized body.
    // Once admitted, malformed alphabet/padding remains a contract error.
    if decoded_len > max_bytes {
        return Ok(skipped(ContentSkipReason::SizeLimitExceeded));
    }
    let mut bytes = vec![0; decoded_len];
    STANDARD
        .decode_slice(encoded, &mut bytes)
        .map_err(|_| ContentPolicyError::MalformedBase64)?;
    Ok(classify_bytes(&bytes, max_bytes))
}

fn classify_bytes(bytes: &[u8], max_bytes: usize) -> ContentDisposition {
    // BOM detection precedes NUL/control detection: UTF-16 text contains NULs.
    if let Some(body) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return decode_utf16(body, true, max_bytes);
    }
    if let Some(body) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return decode_utf16(body, false, max_bytes);
    }
    if has_binary_signature(bytes) || bytes.contains(&0) {
        return skipped(ContentSkipReason::UnsupportedBinary);
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => classify_text(text),
        Err(_) => skipped(ContentSkipReason::UnsupportedEncoding),
    }
}

fn classify_text(text: &str) -> ContentDisposition {
    if text.trim().is_empty() {
        skipped(ContentSkipReason::EmptyContent)
    } else if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\u{c}'))
    {
        skipped(ContentSkipReason::UnsupportedBinary)
    } else {
        ContentDisposition::Text(text.to_owned())
    }
}

fn decode_utf16(bytes: &[u8], little_endian: bool, max_bytes: usize) -> ContentDisposition {
    if !bytes.len().is_multiple_of(2) {
        return skipped(ContentSkipReason::UnsupportedEncoding);
    }
    let chars = || {
        char::decode_utf16(bytes.chunks_exact(2).map(|pair| {
            let pair = [pair[0], pair[1]];
            if little_endian {
                u16::from_le_bytes(pair)
            } else {
                u16::from_be_bytes(pair)
            }
        }))
    };
    // Size and validate first so UTF-8 expansion cannot grow an allocation past
    // the ceiling. No intermediate Vec<u16> or lossy replacement characters.
    let mut size = 0usize;
    for character in chars() {
        let Ok(character) = character else {
            return skipped(ContentSkipReason::UnsupportedEncoding);
        };
        let Some(next) = size
            .checked_add(character.len_utf8())
            .filter(|n| *n <= max_bytes)
        else {
            return skipped(ContentSkipReason::SizeLimitExceeded);
        };
        size = next;
    }
    let mut text = String::with_capacity(size);
    for character in chars() {
        // The immutable input was validated above; propagate defensively without
        // converting malformed transport bytes into text.
        let Ok(character) = character else {
            return skipped(ContentSkipReason::UnsupportedEncoding);
        };
        text.push(character);
    }
    if text.trim().is_empty() {
        skipped(ContentSkipReason::EmptyContent)
    } else if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\u{c}'))
    {
        skipped(ContentSkipReason::UnsupportedBinary)
    } else {
        ContentDisposition::Text(text)
    }
}

fn has_binary_signature(bytes: &[u8]) -> bool {
    const SIGNATURES: &[&[u8]] = &[
        b"\x89PNG\r\n\x1a\n",
        b"\0\0\x01\0",
        b"\0\0\x02\0", // PNG, ICO/CUR
        b"wOFF",
        b"wOF2",
        b"OTTO",
        b"\0\x01\0\0",
        b"ttcf", // fonts
        b"\xfd7zXZ\0",
        b"\x1f\x8b",
        b"PK\x03\x04", // archives
        b"ID3",
        b"OggS",
        b"fLaC", // audio
        b"%PDF-",
        b"\x7fELF",
        b"\xff\xd8\xff",
        b"GIF87a",
        b"GIF89a",
    ];
    SIGNATURES
        .iter()
        .any(|signature| bytes.starts_with(signature))
        || (bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0)
}

#[cfg(test)]
#[path = "content_policy_tests.rs"]
mod tests;
