//! Sub-stage timing instrumentation for the `ask` pipeline.
//!
//! Capture is gated on `cfg.ask_diagnostics` so the no-diagnostics path stays
//! free of `Instant` probes; that invariant is encoded in the type — the
//! `Disabled` variant has no slots, so probes that would write to a slot are
//! statically no-ops.

use std::time::Instant;

/// Mutable accumulator for ask sub-stage timings. Threaded by `&mut` through
/// the pipeline.
#[derive(Debug, Clone)]
pub(crate) enum AskTiming {
    /// Diagnostics off — only carries `request_start` for the disabled-eval
    /// path so the helper can be constructed without `Some(Instant)` plumbing.
    Disabled { request_start: Option<Instant> },
    /// Diagnostics on — every slot captures.
    Enabled(Box<EnabledAskTiming>),
}

#[derive(Debug, Clone)]
pub(crate) struct EnabledAskTiming {
    /// Request start time captured at the CLI dispatch boundary, used as the
    /// TTFT origin for user-visible LLM latency.
    pub request_start: Instant,
    pub tei_embed_ms: Option<u128>,
    pub qdrant_primary_ms: Option<u128>,
    pub qdrant_secondary_ms: Option<u128>,
    pub rerank_ms: Option<u128>,
    pub top_select_ms: Option<u128>,
    pub full_doc_fetch_ms: Option<u128>,
    pub supplemental_ms: Option<u128>,
    pub llm_ttft_ms: Option<u128>,
    pub llm_total_ms: Option<u128>,
    pub streamed: Option<bool>,
    pub normalize_ms: Option<u128>,
}

impl AskTiming {
    pub(crate) fn new(diagnostics_enabled: bool, request_start: Instant) -> Self {
        if diagnostics_enabled {
            AskTiming::Enabled(Box::new(EnabledAskTiming {
                request_start,
                tei_embed_ms: None,
                qdrant_primary_ms: None,
                qdrant_secondary_ms: None,
                rerank_ms: None,
                top_select_ms: None,
                full_doc_fetch_ms: None,
                supplemental_ms: None,
                llm_ttft_ms: None,
                llm_total_ms: None,
                streamed: None,
                normalize_ms: None,
            }))
        } else {
            AskTiming::Disabled {
                request_start: Some(request_start),
            }
        }
    }

    /// Disabled accumulator with no request_start (used by paths like
    /// `evaluate` that don't emit ask sub-stage timing).
    pub(crate) fn disabled() -> Self {
        AskTiming::Disabled {
            request_start: None,
        }
    }

    /// Returns the captured request-start `Instant` when one exists.
    pub(crate) fn request_start(&self) -> Option<Instant> {
        match self {
            AskTiming::Disabled { request_start } => *request_start,
            AskTiming::Enabled(e) => Some(e.request_start),
        }
    }

    /// Borrow the enabled inner state when diagnostics is on.
    pub(crate) fn enabled(&self) -> Option<&EnabledAskTiming> {
        match self {
            AskTiming::Enabled(e) => Some(e.as_ref()),
            AskTiming::Disabled { .. } => None,
        }
    }

    pub(crate) fn record(&mut self, slot: AskTimingSlot, t: Instant) {
        if matches!(self, AskTiming::Disabled { .. }) {
            return;
        }
        let v = t.elapsed().as_millis();
        self.set(slot, v);
    }

    pub(crate) fn set(&mut self, slot: AskTimingSlot, v: u128) {
        let AskTiming::Enabled(e) = self else {
            return;
        };
        match slot {
            AskTimingSlot::TeiEmbed => e.tei_embed_ms = Some(v),
            AskTimingSlot::QdrantPrimary => e.qdrant_primary_ms = Some(v),
            AskTimingSlot::QdrantSecondary => e.qdrant_secondary_ms = Some(v),
            AskTimingSlot::Rerank => e.rerank_ms = Some(v),
            AskTimingSlot::TopSelect => e.top_select_ms = Some(v),
            AskTimingSlot::FullDocFetch => e.full_doc_fetch_ms = Some(v),
            AskTimingSlot::Supplemental => e.supplemental_ms = Some(v),
            AskTimingSlot::LlmTotal => e.llm_total_ms = Some(v),
            AskTimingSlot::Normalize => e.normalize_ms = Some(v),
        }
    }

    pub(crate) fn set_ttft(&mut self, ttft_ms: u128) {
        if let AskTiming::Enabled(e) = self {
            e.llm_ttft_ms = Some(ttft_ms);
        }
    }

    pub(crate) fn set_streamed(&mut self, streamed: bool) {
        if let AskTiming::Enabled(e) = self {
            e.streamed = Some(streamed);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum AskTimingSlot {
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
#[path = "timing_tests.rs"]
mod tests;
