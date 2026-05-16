//! Size-based log file rotation.
//!
//! Wraps a single log file under `<dir>/<file_name>` and rotates it when the
//! configured byte budget is exceeded. Rotated archives are renamed in
//! descending order: `<file_name>.1`, `<file_name>.2`, ..., up to
//! `max_files`. The oldest archive is deleted on rotation.
//!
//! Designed for use with `tracing_appender::non_blocking`, which serialises
//! writes through a single dedicated worker thread — this writer therefore
//! does not perform internal locking.
//!
//! Configuration knobs (parsed by [`init_tracing`]):
//! - `max_bytes = 0` is treated as "never rotate" (single file grows
//!   unboundedly).
//! - `max_files = 0` deletes the active log on rotation without keeping any
//!   archive (effectively a high-water-mark truncate).
//! - `max_files = 1` keeps only `<file_name>.1`.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

pub(crate) struct SizeRotatingFile {
    dir: PathBuf,
    file_name: String,
    max_bytes: u64,
    max_files: usize,
    current: File,
    current_size: u64,
}

impl SizeRotatingFile {
    pub(crate) fn new(
        dir: PathBuf,
        file_name: String,
        max_bytes: u64,
        max_files: usize,
    ) -> io::Result<Self> {
        crate::core::paths::ensure_private_dir(&dir)?;
        let path = dir.join(&file_name);
        let current = OpenOptions::new().create(true).append(true).open(&path)?;
        let current_size = current.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            dir,
            file_name,
            max_bytes,
            max_files,
            current,
            current_size,
        })
    }

    fn active_path(&self) -> PathBuf {
        self.dir.join(&self.file_name)
    }

    fn archive_path(&self, n: usize) -> PathBuf {
        self.dir.join(format!("{}.{n}", self.file_name))
    }

    /// Rotate: shift `<file>.{N-1}` → `<file>.N` from the top down,
    /// `<file>` → `<file>.1`, drop archives beyond `max_files`,
    /// then reopen the active file fresh.
    fn rotate(&mut self) -> io::Result<()> {
        // Flush any pending bytes on the file handle before renaming.
        let _ = self.current.flush();

        // Drop archives that would exceed the retention budget. When
        // max_files == 0 we discard the active file entirely below.
        if self.max_files >= 1 {
            // Remove the oldest if it exists; we are about to shift everyone down.
            let oldest = self.archive_path(self.max_files);
            if oldest.exists() {
                let _ = std::fs::remove_file(&oldest);
            }
            // Shift n-1 → n, n-2 → n-1, ... 1 → 2.
            for n in (1..self.max_files).rev() {
                let from = self.archive_path(n);
                let to = self.archive_path(n + 1);
                if from.exists() {
                    let _ = std::fs::rename(&from, &to);
                }
            }
            // active → .1
            let active = self.active_path();
            if active.exists() {
                let _ = std::fs::rename(&active, self.archive_path(1));
            }
        } else {
            // max_files == 0: just delete the active file; no archive kept.
            let active = self.active_path();
            if active.exists() {
                let _ = std::fs::remove_file(&active);
            }
        }

        // Reopen fresh.
        let new_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.active_path())?;
        self.current = new_file;
        self.current_size = 0;
        Ok(())
    }
}

impl Write for SizeRotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.max_bytes > 0
            && self.current_size > 0
            && self.current_size.saturating_add(buf.len() as u64) > self.max_bytes
        {
            // Best-effort rotate; if it fails we still try to write to the current file.
            let _ = self.rotate();
        }
        let n = self.current.write(buf)?;
        self.current_size = self.current_size.saturating_add(n as u64);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.current.flush()
    }
}

#[cfg(test)]
#[path = "size_rotating_tests.rs"]
mod tests;
