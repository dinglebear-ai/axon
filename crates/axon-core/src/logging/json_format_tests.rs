use super::JsonFormat;
use std::io;
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::prelude::*;

#[derive(Clone, Default)]
struct BufWriter(Arc<Mutex<Vec<u8>>>);

impl io::Write for BufWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for BufWriter {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[test]
fn file_json_log_layer_redacts_secret_before_write() {
    // Fail-closed: the file JSON log layer (`~/.axon/logs/axon.log`) must
    // redact the same way the console layer does. Previously it used the
    // stock `tracing_subscriber` JSON formatter directly, bypassing
    // `redact_event_fields` entirely — a secret logged via
    // `tracing::warn!(token = %secret, ...)` would be scrubbed on the
    // console but written to disk unredacted. This drives a real
    // `tracing::Event` through `JsonFormat` (the layer now installed for the
    // file sink) and inspects the actual bytes written, proving the secret
    // never reaches the sink.
    let buf = BufWriter::default();
    let layer = tracing_subscriber::fmt::layer()
        .event_format(JsonFormat)
        .with_ansi(false)
        .with_writer(buf.clone());
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        tracing::warn!(
            detail = "token=deadbeefdeadbeefdeadbeefdeadbeef",
            "upstream error: Authorization: Bearer abcdef0123456789abcdef"
        );
    });

    let written = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
    assert!(
        !written.contains("abcdef0123456789abcdef"),
        "secret leaked into file JSON log: {written}"
    );
    assert!(
        !written.contains("deadbeefdeadbeefdeadbeefdeadbeef"),
        "secret leaked into file JSON log: {written}"
    );
    assert!(written.contains(crate::redact::REDACTION_PLACEHOLDER));
}

#[test]
fn file_json_log_layer_redacts_span_fields() {
    // Fix 1 counterpart for the file JSON sink: span-level fields flow into
    // the JSON "spans" array via the same `collect_span_fields` helper the
    // console formatter uses. Prove a secret-shaped span field never reaches
    // the on-disk JSON line either.
    let buf = BufWriter::default();
    let layer = tracing_subscriber::fmt::layer()
        .event_format(JsonFormat)
        .with_ansi(false)
        .with_writer(buf.clone());
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!(
            "request",
            header = "Authorization: Bearer abcdef0123456789abcdef"
        );
        let _guard = span.enter();
        tracing::warn!("handling request");
    });

    let written = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
    assert!(
        !written.contains("abcdef0123456789abcdef"),
        "secret leaked into file JSON log spans: {written}"
    );
    assert!(written.contains(crate::redact::REDACTION_PLACEHOLDER));
}
