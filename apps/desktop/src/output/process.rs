use std::io::{self, Read};
use std::process::{Command, Stdio};
use std::thread;

use super::{BoundedProcessOutput, OUTPUT_LIMIT, TRUNCATED_MESSAGE};

pub(crate) fn run_command_bounded(mut command: Command) -> io::Result<BoundedProcessOutput> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("child stdout was not piped"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("child stderr was not piped"))?;

    let stdout_reader = thread::spawn(move || read_bounded(stdout));
    let stderr_reader = thread::spawn(move || read_bounded(stderr));
    let status = child.wait()?;
    let stdout = stdout_reader
        .join()
        .map_err(|_| io::Error::other("stdout reader panicked"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| io::Error::other("stderr reader panicked"))??;

    Ok(BoundedProcessOutput {
        status,
        stdout,
        stderr,
    })
}

fn read_bounded(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let mut buffer = BoundedByteBuffer::new(OUTPUT_LIMIT);
    let mut chunk = [0_u8; 8192];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.push(&chunk[..read]);
    }
    Ok(buffer.into_bytes())
}

pub(super) struct BoundedByteBuffer {
    bytes: Vec<u8>,
    limit: usize,
    truncated: bool,
}

impl BoundedByteBuffer {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(8192)),
            limit,
            truncated: false,
        }
    }

    pub(super) fn push(&mut self, chunk: &[u8]) {
        let remaining = self.limit.saturating_sub(self.bytes.len());
        if self.bytes.len() < self.limit {
            self.bytes
                .extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        }
        if chunk.len() > remaining {
            self.truncated = true;
        }
    }

    pub(super) fn into_bytes(mut self) -> Vec<u8> {
        if !self.truncated {
            return self.bytes;
        }

        let boundary = valid_utf8_boundary(&self.bytes);
        self.bytes.truncate(boundary);
        self.bytes.extend_from_slice(TRUNCATED_MESSAGE.as_bytes());
        self.bytes
    }
}

fn valid_utf8_boundary(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(error) => error.valid_up_to(),
    }
}
