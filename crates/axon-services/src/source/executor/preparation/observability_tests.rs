use super::*;
use axon_api::source::*;
use std::sync::{Arc, Mutex};
use tracing::{
    Event, Metadata, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
};

#[test]
fn arbitrary_method_metadata_is_not_logged() {
    assert_eq!(
        method_label("source body contains credentials or prose"),
        "unrecognized"
    );
    assert_eq!(method_label("tree_sitter"), "tree_sitter");
}

#[derive(Clone)]
struct Capture(Arc<Mutex<String>>);
impl Visit for Capture {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write;
        write!(&mut *self.0.lock().unwrap(), "{}={value:?};", field.name()).unwrap();
    }
}
impl Subscriber for Capture {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, event: &Event<'_>) {
        event.record(&mut self.clone());
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

#[test]
fn invalid_supplied_status_or_grammar_does_not_reach_preparation_logs() {
    let result = PrepareSourceDocumentResult::Skipped(SkippedDocument {
        document_id: DocumentId::from("observed-doc"),
        source_id: SourceId::from("src"),
        source_item_key: SourceItemKey::from("item"),
        generation: SourceGenerationId::from("gen"),
        reason: ContentSkipReason::EmptyContent,
    });
    for (status, grammar) in [
        ("injected-status-body", "rust"),
        ("parsed", "injected-grammar-body"),
    ] {
        let observation = PreparationObservation::from_value(&serde_json::json!({
            "code_ast_status": status, "code_grammar": grammar,
            "code_symbol_count":0, "symbol_extraction_status":"ast"
        }));
        let captured = Capture(Arc::new(Mutex::new(String::new())));
        tracing::subscriber::with_default(captured.clone(), || {
            trace_document(&result, &observation)
        });
        let output = captured.0.lock().unwrap();
        assert!(output.contains("document preparation outcome"));
        assert!(output.contains("not_observed"));
        assert!(!output.contains("injected-status-body"));
        assert!(!output.contains("injected-grammar-body"));
    }
}
