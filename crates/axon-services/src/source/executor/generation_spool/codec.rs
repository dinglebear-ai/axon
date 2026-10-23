//! Bounded record encoding for transactional generation work spools.
use std::io::{self, Write};

const OVERSIZE: &str = "generation spool record exceeds configured byte window";

struct BoundedBuffer {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let end = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|&end| end <= self.limit)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, OVERSIZE))?;
        reserve_bounded(&mut self.bytes, end, self.limit)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// Geometric growth avoids repeated small reallocations while never reserving
// beyond the admitted record window.
fn reserve_bounded(bytes: &mut Vec<u8>, required: usize, limit: usize) -> io::Result<()> {
    if required > bytes.capacity() {
        let target = bytes
            .capacity()
            .saturating_mul(2)
            .max(4096)
            .min(limit)
            .max(required);
        bytes
            .try_reserve_exact(target - bytes.len())
            .map_err(io::Error::other)?;
    }
    Ok(())
}

pub(super) fn encode(value: &impl serde::Serialize, limit: usize) -> anyhow::Result<Vec<u8>> {
    let mut buffer = BoundedBuffer {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut buffer, value)?;
    Ok(buffer.bytes)
}
