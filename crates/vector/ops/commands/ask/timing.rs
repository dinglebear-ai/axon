//! Sub-stage timing instrumentation for the `ask` pipeline.
//!
//! A single [`AskTiming`] struct is threaded through the ask pipeline
//! (`build_ask_context` → `retrieve_ask_candidates` / `build_context_from_candidates`
//! → `ask_llm_answer` → `streaming::run_streaming_completion`). Each stage records
//! its own bucket. The aggregator in `ask.rs` folds the struct into the JSON
//! payload alongside the legacy 5-bucket `timing_ms` shape.
//!
//! Capture is gated on `cfg.ask_diagnostics`: when disabled, the struct is still
//! threaded but every `Option<u128>` field stays `None` (and is omitted from JSON
//! via `skip_serializing_if`). Instant probes only fire when diagnostics is on,
//! keeping the no-diagnostics path free of instrumentation overhead.
//!
//! See bd `axon_rust-nm9` for design rationale.

use std::time::Instant;

/// Mutable accumulator for ask sub-stage timings. Threaded by `&mut` through
/// the pipeline. `request_start` is captured at the CLI entry boundary so
/// `llm_ttft_ms` includes ACP cold-start, not just retrieval-onwards latency.
#[derive(Debug, Clone)]
pub(crate) struct AskTiming {
    /// True when diagnostic capture is enabled; false skips Instant probes.
    pub diagnostics_enabled: bool,
    /// Request start time — captured at CLI dispatch boundary (run_ask), used
    /// as the TTFT origin so the cold-start ACP tax is included.
    pub request_start: Instant,

    pub warm_session_ready_ms: Option<u128>,
    pub tei_embed_ms: Option<u128>,
    pub qdrant_primary_ms: Option<u128>,
    pub qdrant_secondary_ms: Option<u128>,
    pub rerank_ms: Option<u128>,
    pub top_select_ms: Option<u128>,
    pub full_doc_fetch_ms: Option<u128>,
    pub supplemental_ms: Option<u128>,
    pub llm_ttft_ms: Option<u128>,
    pub llm_total_ms: Option<u128>,
    pub llm_warm_path: Option<bool>,
    pub normalize_ms: Option<u128>,
}

impl AskTiming {
    pub(crate) fn new(diagnostics_enabled: bool, request_start: Instant) -> Self {
        Self {
            diagnostics_enabled,
            request_start,
            warm_session_ready_ms: None,
            tei_embed_ms: None,
            qdrant_primary_ms: None,
            qdrant_secondary_ms: None,
            rerank_ms: None,
            top_select_ms: None,
            full_doc_fetch_ms: None,
            supplemental_ms: None,
            llm_ttft_ms: None,
            llm_total_ms: None,
            llm_warm_path: None,
            normalize_ms: None,
        }
    }

    /// Capture-helper: records the elapsed time from `t` into `slot` only when
    /// diagnostics is on. No-op otherwise.
    pub(crate) fn record(&mut self, slot: AskTimingSlot, t: Instant) {
        if !self.diagnostics_enabled {
            return;
        }
        let v = t.elapsed().as_millis();
        self.set(slot, v);
    }

    pub(crate) fn set(&mut self, slot: AskTimingSlot, v: u128) {
        if !self.diagnostics_enabled {
            return;
        }
        match slot {
            AskTimingSlot::WarmSessionReady => self.warm_session_ready_ms = Some(v),
            AskTimingSlot::TeiEmbed => self.tei_embed_ms = Some(v),
            AskTimingSlot::QdrantPrimary => self.qdrant_primary_ms = Some(v),
            AskTimingSlot::QdrantSecondary => self.qdrant_secondary_ms = Some(v),
            AskTimingSlot::Rerank => self.rerank_ms = Some(v),
            AskTimingSlot::TopSelect => self.top_select_ms = Some(v),
            AskTimingSlot::FullDocFetch => self.full_doc_fetch_ms = Some(v),
            AskTimingSlot::Supplemental => self.supplemental_ms = Some(v),
            AskTimingSlot::LlmTotal => self.llm_total_ms = Some(v),
            AskTimingSlot::Normalize => self.normalize_ms = Some(v),
        }
    }

    pub(crate) fn set_warm_path(&mut self, warm_path: bool) {
        if self.diagnostics_enabled {
            self.llm_warm_path = Some(warm_path);
        }
    }

    pub(crate) fn set_ttft(&mut self, ttft_ms: u128) {
        if self.diagnostics_enabled {
            self.llm_ttft_ms = Some(ttft_ms);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum AskTimingSlot {
    WarmSessionReady,
    TeiEmbed,
    QdrantPrimary,
    QdrantSecondary,
    Rerank,
    TopSelect,
    FullDocFetch,
    Supplemental,
    LlmTotal,
    Normalize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_record_is_noop() {
        let mut t = AskTiming::new(false, Instant::now());
        let probe = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(2));
        t.record(AskTimingSlot::TeiEmbed, probe);
        assert!(t.tei_embed_ms.is_none());
        t.set_ttft(42);
        assert!(t.llm_ttft_ms.is_none());
        t.set_warm_path(true);
        assert!(t.llm_warm_path.is_none());
    }

    #[test]
    fn enabled_record_populates() {
        let mut t = AskTiming::new(true, Instant::now());
        let probe = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(2));
        t.record(AskTimingSlot::TeiEmbed, probe);
        assert!(t.tei_embed_ms.is_some_and(|v| v >= 1));
        t.set_warm_path(false);
        assert_eq!(t.llm_warm_path, Some(false));
        t.set_ttft(99);
        assert_eq!(t.llm_ttft_ms, Some(99));
    }
}
