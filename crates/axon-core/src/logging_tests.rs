use super::should_use_ansi_for_writer;
use crate::config::ColorChoice;
use crate::ui::{COLOR_OVERRIDE, COLOR_TEST_GUARD, install_color_choice};
use std::io;
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::prelude::*;

#[test]
fn color_override_controls_logging_ansi() {
    let _g = COLOR_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let prev = COLOR_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);

    install_color_choice(ColorChoice::Never);
    assert!(!should_use_ansi_for_writer(true));

    install_color_choice(ColorChoice::Always);
    assert!(should_use_ansi_for_writer(false));

    install_color_choice(ColorChoice::Auto);
    assert!(
        !crate::ui::color_forced_always(),
        "auto must clear the forced-always override"
    );

    COLOR_OVERRIDE.store(prev, std::sync::atomic::Ordering::Relaxed);
}

#[test]
fn redact_event_fields_scrubs_message_and_extra() {
    let mut v = super::EventVisitor {
        message: "auth failed: Authorization: Bearer abcdef0123456789abcdef".to_string(),
        extra: vec![(
            "cause".to_string(),
            "token=deadbeefdeadbeefdeadbeefdeadbeef".to_string(),
        )],
    };
    super::redact_event_fields(&mut v);
    assert!(!v.message.contains("abcdef0123456789abcdef"));
    assert!(!v.extra[0].1.contains("deadbeefdeadbeefdeadbeef"));
}

#[test]
fn redact_event_fields_caps_oversized_values() {
    // Fix 2: the logging redaction path must not run an unbounded regex scan
    // over an arbitrarily large field value — mirrors the fail-closed size
    // cap `redact::boundary::redact_text_checked` applies on the durable
    // write path (`MAX_REDACTABLE_TEXT_BYTES`). A single call site logging a
    // huge blob (e.g. a full response body) must be capped, not fully
    // scanned, on the hot logging path.
    let oversized = "a".repeat(super::MAX_LOGGED_FIELD_BYTES + 1);
    let mut v = super::EventVisitor {
        message: oversized.clone(),
        extra: vec![("body".to_string(), oversized)],
    };
    super::redact_event_fields(&mut v);
    assert_eq!(v.message, super::OVERSIZED_FIELD_PLACEHOLDER);
    assert_eq!(v.extra[0].1, super::OVERSIZED_FIELD_PLACEHOLDER);
}

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
fn span_fields_are_redacted_on_console_output() {
    // Fix 1: span-level fields (e.g. from a future
    // `#[tracing::instrument(fields(...))]`) must be scrubbed identically to
    // event-level fields. Previously `collect_span_fields` returned the raw
    // formatted span field strings with no redaction call at all, so a
    // secret-shaped span field leaked on both console and file-JSON output.
    let buf = BufWriter::default();
    let layer = tracing_subscriber::fmt::layer()
        .event_format(super::CliFormat)
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
        "secret leaked into console span fields: {written}"
    );
    assert!(written.contains(crate::redact::REDACTION_PLACEHOLDER));
}
