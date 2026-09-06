use std::io::{self, Write};

pub(crate) fn write_chunk(writer: &mut impl Write, text: &str) -> io::Result<bool> {
    match writer
        .write_all(text.as_bytes())
        .and_then(|()| writer.flush())
    {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(false),
        Err(error) => Err(error),
    }
}

pub(crate) fn finish_line(writer: &mut impl Write) -> io::Result<bool> {
    match writeln!(writer).and_then(|()| writer.flush()) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "stream_output_tests.rs"]
mod tests;
