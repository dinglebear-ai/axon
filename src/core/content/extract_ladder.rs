//! Multi-strategy DOM extraction retry ladder (axon_rust-jh32).
//!
//! Ports webclaw `lib.rs:144-177`. Inserts as Tier-1.5 between the HTTP
//! fetch and Chrome auto-switch escalation: pages that produce thin
//! markdown via the scored extractor get up to two cheaper retries
//! before paying for Chrome.
//!
//! ## Strategies
//! - **Scored** (tier 0): user's selector + main_content scoring.
//! - **Relaxed** (tier 1): if scored < `strategy1` words AND a user
//!   `root_selector` was active, retry without it.
//! - **Body** (tier 2): if still thin AND no user selector, retry with
//!   `main_content: false` (raw body minus boilerplate). Wins only if
//!   it produces > `body_multiplier × prev_words` (default 2.0) AND > 50 words.
//!
//! ## Body-byte probe gate
//! Cheap byte-length scan of the raw `<body>` region; < 5 KiB skips both
//! retries. Prevents 2–3× DOM walks on genuinely-empty pages.
//!
//! ## Invariants (DO NOT change)
//! `readability: false` and `clean_html: false` from `markdown.rs` are
//! production-confirmed regressions when flipped. Body tier relaxes only
//! `main_content`.

use crate::core::content::markdown::{
    BOILERPLATE_SELECTORS, bytes_to_markdown, clean_markdown_whitespace,
};
use spider_transformations::transformation::content::{
    ReturnFormat, SelectorConfiguration, TransformConfig, TransformInput, transform_content_input,
};
use std::sync::LazyLock;

const BODY_BYTE_PROBE_THRESHOLD: usize = 5 * 1024;
const BODY_TIER_MIN_WORDS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LadderTier {
    Scored,
    Relaxed,
    Body,
}

impl LadderTier {
    pub fn as_str(self) -> &'static str {
        match self {
            LadderTier::Scored => "scored",
            LadderTier::Relaxed => "relaxed",
            LadderTier::Body => "body",
        }
    }
}

#[derive(Debug)]
pub struct LadderResult {
    pub markdown: String,
    pub tier: LadderTier,
    pub word_count: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct LadderThresholds {
    pub strategy1: usize,
    pub strategy2: usize,
    pub body_multiplier: f64,
}

impl LadderThresholds {
    pub fn from_config(cfg: &crate::core::config::Config) -> Self {
        Self {
            strategy1: cfg.ladder_word_threshold_strategy1,
            strategy2: cfg.ladder_word_threshold_strategy2,
            body_multiplier: cfg.ladder_body_multiplier,
        }
    }
}

static TRANSFORM_CONFIG_BODY: LazyLock<TransformConfig> = LazyLock::new(|| TransformConfig {
    return_format: ReturnFormat::Markdown,
    readability: false,
    clean_html: false,
    main_content: false,
    filter_images: true,
    filter_svg: true,
});

pub fn extract_with_ladder(
    html: &[u8],
    selector_config: Option<&SelectorConfiguration>,
    thresholds: LadderThresholds,
) -> LadderResult {
    let scored = bytes_to_markdown(html, selector_config);
    let scored_words = word_count(&scored);

    if approximate_body_bytes(html) < BODY_BYTE_PROBE_THRESHOLD {
        return LadderResult {
            markdown: scored,
            tier: LadderTier::Scored,
            word_count: scored_words,
        };
    }

    let user_had_root_selector = selector_config.is_some_and(has_root_selector);

    let mut best_md = scored;
    let mut best_words = scored_words;
    let mut best_tier = LadderTier::Scored;

    if best_words < thresholds.strategy1 && user_had_root_selector {
        let relaxed = bytes_to_markdown(html, None);
        let relaxed_words = word_count(&relaxed);
        if relaxed_words > best_words {
            best_md = relaxed;
            best_words = relaxed_words;
            best_tier = LadderTier::Relaxed;
        }
    }

    if best_words < thresholds.strategy2 && !user_had_root_selector {
        let body_md = extract_body_only(html);
        let body_words = word_count(&body_md);
        let multiplier_ok = (body_words as f64) > (best_words as f64) * thresholds.body_multiplier;
        let floor_ok = body_words > BODY_TIER_MIN_WORDS;
        if multiplier_ok && floor_ok {
            best_md = body_md;
            best_words = body_words;
            best_tier = LadderTier::Body;
        }
    }

    LadderResult {
        markdown: best_md,
        tier: best_tier,
        word_count: best_words,
    }
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

fn approximate_body_bytes(html: &[u8]) -> usize {
    let s = match std::str::from_utf8(html) {
        Ok(s) => s,
        Err(_) => return html.len(),
    };
    let lower_head_end = s.len().min(8192);
    let head_lower = s.get(..lower_head_end).unwrap_or("").to_lowercase();
    let start = match head_lower.find("<body") {
        Some(i) => i,
        None => return html.len(),
    };

    let tail_start = s.len().saturating_sub(8192);
    let tail_lower = s.get(tail_start..).unwrap_or("").to_lowercase();
    let end = match tail_lower.rfind("</body>") {
        Some(i) => tail_start + i,
        None => return html.len(),
    };

    end.saturating_sub(start)
}

fn extract_body_only(html: &[u8]) -> String {
    let input = TransformInput {
        url: None,
        content: html,
        screenshot_bytes: None,
        encoding: None,
        selector_config: None,
        ignore_tags: Some(BOILERPLATE_SELECTORS),
    };
    let raw = transform_content_input(input, &TRANSFORM_CONFIG_BODY);
    clean_markdown_whitespace(raw.trim())
}

fn has_root_selector(sc: &SelectorConfiguration) -> bool {
    sc.root_selector.is_some()
}

#[cfg(test)]
#[path = "extract_ladder_tests.rs"]
mod tests;
