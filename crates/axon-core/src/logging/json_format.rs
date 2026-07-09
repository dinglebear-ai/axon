//! JSON-line event formatter for the file log sink.
//!
//! Reuses the same `EventVisitor` + `redact_event_fields` fail-closed scrub
//! as `CliFormat` (see its doc comment in `logging.rs`) so a secret redacted
//! on the console is also redacted on disk — the stock `tracing_subscriber`
//! JSON formatter this replaces did not funnel through `redact_event_fields`
//! at all, so a secret logged via `tracing::warn!(token = %secret, ...)`
//! would be scrubbed on the console but written to `~/.axon/logs/axon.log`
//! unredacted.

use std::fmt;

use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields, format::Writer};
use tracing_subscriber::registry::LookupSpan;

use super::{EventVisitor, collect_span_fields, redact_event_fields};

pub(super) struct JsonFormat;

impl<S, N> FormatEvent<S, N> for JsonFormat
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> fmt::Result {
        let mut v = EventVisitor::default();
        event.record(&mut v);
        redact_event_fields(&mut v);

        let metadata = event.metadata();
        let mut line = serde_json::Map::new();
        line.insert(
            "timestamp".to_string(),
            serde_json::Value::String(chrono::Utc::now().to_rfc3339()),
        );
        line.insert(
            "level".to_string(),
            serde_json::Value::String(metadata.level().to_string()),
        );
        line.insert(
            "target".to_string(),
            serde_json::Value::String(metadata.target().to_string()),
        );
        line.insert("message".to_string(), serde_json::Value::String(v.message));
        for (key, val) in v.extra {
            line.insert(key, serde_json::Value::String(val));
        }
        let spans = collect_span_fields(ctx);
        if !spans.is_empty() {
            line.insert(
                "spans".to_string(),
                serde_json::Value::Array(
                    spans.into_iter().map(serde_json::Value::String).collect(),
                ),
            );
        }

        writeln!(writer, "{}", serde_json::Value::Object(line))
    }
}

#[cfg(test)]
#[path = "json_format_tests.rs"]
mod tests;
