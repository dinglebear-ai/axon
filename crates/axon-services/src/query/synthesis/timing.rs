//! Sub-stage timing instrumentation for the `ask` synthesis pipeline.
//!
//! Ported verbatim from legacy `axon_vector::ops::commands::ask::timing`.
//! Capture is gated on `cfg.ask_diagnostics` so the no-diagnostics path stays
//! free of `Instant` probes; that invariant is encoded in the type — the
//! `Disabled` variant has no slots, so probes that would write to a slot are
//! statically no-ops.

use std::time::Instant;

/// Mutable accumulator for ask sub-stage timings. Threaded by `&mut` through
/// the pipeline.
#[derive(Debug, Clone)]
pub(crate) enum AskTiming {
    /// Diagnostics off — carries `request_start` plus the two fields that are
    /// always emitted in the timing line (`streamed`, `llm_ttft_ms`).
    Disabled {
        request_start: Option<Instant>,
        streamed: Option<bool>,
        llm_ttft_ms: Option<u128>,
    },
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
                streamed: None,
                llm_ttft_ms: None,
            }
        }
    }

    /// Disabled accumulator with no request_start (used by paths like
    /// `evaluate` that don't emit ask sub-stage timing).
    #[cfg(test)]
    pub(crate) fn disabled() -> Self {
        AskTiming::Disabled {
            request_start: None,
            streamed: None,
            llm_ttft_ms: None,
        }
    }

    /// Returns the captured request-start `Instant` when one exists.
    pub(crate) fn request_start(&self) -> Option<Instant> {
        match self {
            AskTiming::Disabled { request_start, .. } => *request_start,
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
            AskTimingSlot::LlmTotal => e.llm_total_ms = Some(v),
            AskTimingSlot::Normalize => e.normalize_ms = Some(v),
        }
    }

    pub(crate) fn set_ttft(&mut self, ttft_ms: u128) {
        match self {
            AskTiming::Disabled { llm_ttft_ms, .. } => *llm_ttft_ms = Some(ttft_ms),
            AskTiming::Enabled(e) => e.llm_ttft_ms = Some(ttft_ms),
        }
    }

    pub(crate) fn set_streamed(&mut self, streamed: bool) {
        match self {
            AskTiming::Disabled {
                streamed: s_slot, ..
            } => *s_slot = Some(streamed),
            AskTiming::Enabled(e) => e.streamed = Some(streamed),
        }
    }
}

/// Timing slots settable from the synthesis pipeline (this crate). The
/// retrieval-stage-only slots (`TeiEmbed`, `QdrantPrimary`, `QdrantSecondary`,
/// `Rerank`, `TopSelect`, `FullDocFetch`, `Supplemental`) are legacy-reranker
/// concepts not produced on the `axon-retrieval` engine path, so they are not
/// reproduced here — `EnabledAskTiming` keeps the fields (always `None`) so
/// the wire `AskTiming` shape stays unchanged, but nothing in this crate ever
/// sets them.
#[derive(Debug, Clone, Copy)]
pub(crate) enum AskTimingSlot {
    LlmTotal,
    Normalize,
}

#[cfg(test)]
#[path = "timing_tests.rs"]
mod tests;
