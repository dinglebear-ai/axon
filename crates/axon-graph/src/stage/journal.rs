//! Lossless bounded graph journal codec. No graph fact is discarded.
use super::*;
use base64::Engine as _;
use std::io::Read as _;
const PREFIX: &str = "axon-graph-zlib-v1:";
pub(super) const MAX_RAW_BYTES: u64 = 8 * 1024 * 1024;
pub(super) const MAX_ENCODED_BYTES: u64 = 12 * 1024 * 1024;
pub(super) const MAX_BATCH_BYTES: u64 = 64 * 1024 * 1024;

struct RawLimit<W> {
    writer: W,
    bytes: u64,
}
impl<W: std::io::Write> std::io::Write for RawLimit<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let next = self
            .bytes
            .checked_add(bytes.len() as u64)
            .filter(|next| *next <= MAX_RAW_BYTES)
            .ok_or_else(|| {
                std::io::Error::other("graph journal record exceeds 8 MiB decoded window")
            })?;
        self.writer.write_all(bytes)?;
        self.bytes = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

pub(crate) fn encode(candidate: &GraphCandidate) -> Result<(String, u64)> {
    let mut limited = RawLimit {
        writer: flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast()),
        bytes: 0,
    };
    serde_json::to_writer(&mut limited, candidate).map_err(|error| {
        graph_storage_error(format!("graph journal bounded encode failed: {error}"))
    })?;
    let bytes = limited.writer.finish().map_err(|error| {
        graph_storage_error(format!("graph journal compression failed: {error}"))
    })?;
    let encoded = format!(
        "{PREFIX}{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    );
    if encoded.len() as u64 > MAX_ENCODED_BYTES {
        return Err(graph_storage_error(
            "graph journal encoded record exceeds 12 MiB window",
        ));
    }
    Ok((encoded, limited.bytes))
}

pub(crate) fn decode(encoded: &str) -> Result<GraphCandidate> {
    if encoded.len() as u64 > MAX_ENCODED_BYTES {
        return Err(graph_storage_error(
            "graph journal encoded record exceeds 12 MiB window",
        ));
    }
    if let Some(payload) = encoded.strip_prefix(PREFIX) {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .map_err(|_| graph_storage_error("graph journal compressed payload corrupt"))?;
        let mut decoded = Vec::new();
        let mut decoder = flate2::read::ZlibDecoder::new(bytes.as_slice());
        (&mut decoder)
            .take(MAX_RAW_BYTES + 1)
            .read_to_end(&mut decoded)
            .map_err(|_| graph_storage_error("graph journal decompression failed"))?;
        if decoded.len() as u64 > MAX_RAW_BYTES {
            return Err(graph_storage_error(
                "graph journal record exceeds 8 MiB decoded window",
            ));
        }
        if decoder.total_in() != bytes.len() as u64 {
            return Err(graph_storage_error(
                "graph journal compressed record has trailing bytes",
            ));
        }
        serde_json::from_slice(&decoded)
            .map_err(|_| graph_storage_error("graph journal decoded candidate corrupt"))
    } else {
        if encoded.len() as u64 > MAX_RAW_BYTES {
            return Err(graph_storage_error(
                "legacy graph journal record exceeds decoded window",
            ));
        }
        serde_json::from_str(encoded)
            .map_err(|_| graph_storage_error("legacy graph journal corrupt"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn candidate() -> GraphCandidate {
        serde_json::from_value(serde_json::json!({
            "candidate_id":"candidate", "job_id":"00000000-0000-0000-0000-000000000001",
            "source_id":"source", "source_item_key":"path", "item_canonical_uri":"file:///path",
            "kind":"fixture", "producer":{"adapter":"fixture","version":"1"},
            "nodes":[],"edges":[],"evidence":[],"confidence":1.0,"metadata":{"payload":"x".repeat(100_000)}
        })).unwrap()
    }
    #[test]
    fn compressed_and_legacy_journals_preserve_every_candidate_field() {
        let candidate = candidate();
        let json = serde_json::to_string(&candidate).unwrap();
        let (compressed, raw) = encode(&candidate).unwrap();
        assert_eq!(raw, json.len() as u64);
        assert!(compressed.len() < json.len() / 10);
        assert_eq!(decode(&compressed).unwrap(), candidate);
        assert_eq!(decode(&json).unwrap(), candidate);
    }
    #[test]
    fn decoded_window_and_trailing_compressed_bytes_fail_closed() {
        use std::io::Write as _;
        let mut compressor =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        let chunk = vec![b'x'; 64 * 1024];
        for _ in 0..129 {
            compressor.write_all(&chunk).unwrap();
        }
        let bomb = compressor.finish().unwrap();
        assert!(
            decode(&format!(
                "{PREFIX}{}",
                base64::engine::general_purpose::STANDARD.encode(bomb)
            ))
            .is_err()
        );
        let (valid, _) = encode(&candidate()).unwrap();
        let mut bytes = base64::engine::general_purpose::STANDARD
            .decode(valid.strip_prefix(PREFIX).unwrap())
            .unwrap();
        bytes.extend_from_slice(b"trailing");
        assert!(
            decode(&format!(
                "{PREFIX}{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ))
            .is_err()
        );
        let mut too_large = candidate();
        too_large.metadata.insert(
            "payload".into(),
            serde_json::json!("x".repeat(MAX_RAW_BYTES as usize)),
        );
        assert!(encode(&too_large).is_err());
    }
}
