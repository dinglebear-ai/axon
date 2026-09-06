use super::*;

struct FailingWriter {
    kind: io::ErrorKind,
}

impl Write for FailingWriter {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(self.kind, "injected"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn non_pipe_failure_is_propagated() {
    let error = write_chunk(
        &mut FailingWriter {
            kind: io::ErrorKind::StorageFull,
        },
        "partial",
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::StorageFull);
}

#[test]
fn broken_pipe_stops_stream_without_turning_pipeline_exit_into_failure() {
    assert!(
        !write_chunk(
            &mut FailingWriter {
                kind: io::ErrorKind::BrokenPipe,
            },
            "partial",
        )
        .unwrap()
    );
}
