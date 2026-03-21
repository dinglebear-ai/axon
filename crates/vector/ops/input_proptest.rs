//! Property-based tests for `chunk_text` and `chunk_markdown`.
//!
//! The hand-written tests in `input.rs` cover specific boundary cases.
//! These properties verify invariants across the full input space:
//!
//! **chunk_text** invariants:
//! - Non-empty input always yields at least one chunk.
//! - No chunk exceeds 2000 characters.
//! - Output is deterministic for the same input.
//! - Empty input returns exactly one empty chunk (fast-path behaviour).
//! - All original content is preserved across the chunk sequence.
//!
//! **chunk_markdown** invariants:
//! - No chunk exceeds 2000 characters.
//! - No chunk is empty or whitespace-only (empty chunks are filtered).
//! - Output is deterministic for the same input.
//! - Non-empty, non-whitespace input yields at least one chunk.

use super::{chunk_markdown, chunk_text};
use proptest::prelude::*;

const MAX_CHUNK: usize = 2000;
const OVERLAP: usize = 200;

// ── At-least-one-chunk guarantee ─────────────────────────────────────────────

proptest! {
    /// For any non-empty input string, `chunk_text` must return at least one chunk.
    #[test]
    fn chunk_text_non_empty_input_yields_at_least_one_chunk(
        s in ".{1,10000}",
    ) {
        let chunks = chunk_text(&s);
        prop_assert!(
            !chunks.is_empty(),
            "non-empty input must produce at least one chunk"
        );
    }
}

/// For empty input, `chunk_text` returns exactly one chunk (the fast-path
/// wraps empty string in a vec — documented behaviour).
/// This is a deterministic test, not property-based, but lives here for
/// co-location with the other chunk_text invariants.
#[test]
fn chunk_text_empty_input_returns_one_empty_chunk() {
    let chunks = chunk_text("");
    assert_eq!(
        chunks.len(),
        1,
        "empty input must produce exactly one chunk"
    );
    assert_eq!(
        chunks[0], "",
        "the single chunk for empty input must itself be empty"
    );
}

// ── Maximum chunk size invariant ─────────────────────────────────────────────

proptest! {
    /// No chunk produced by `chunk_text` may exceed 2000 characters.
    /// Uses char count (not byte count) since the function is char-aware.
    #[test]
    fn chunk_text_no_chunk_exceeds_max_chars(
        s in ".{0,10000}",
    ) {
        for (i, chunk) in chunk_text(&s).iter().enumerate() {
            let char_count = chunk.chars().count();
            prop_assert!(
                char_count <= MAX_CHUNK,
                "chunk {i} has {char_count} chars, exceeding MAX={MAX_CHUNK}"
            );
        }
    }
}

proptest! {
    /// Specifically with ASCII-only input (1 byte == 1 char), no chunk byte length
    /// may exceed MAX_CHUNK.  Guards both the char and byte paths simultaneously.
    #[test]
    fn chunk_text_ascii_no_chunk_exceeds_max_bytes(
        s in "[a-zA-Z0-9 ]{0,10000}",
    ) {
        for (i, chunk) in chunk_text(&s).iter().enumerate() {
            prop_assert!(
                chunk.len() <= MAX_CHUNK,
                "ASCII chunk {i} has {} bytes, exceeding MAX={MAX_CHUNK}", chunk.len()
            );
        }
    }
}

// ── Determinism ──────────────────────────────────────────────────────────────

proptest! {
    /// Calling `chunk_text` twice with the same input must produce identical output.
    #[test]
    fn chunk_text_is_deterministic(
        s in ".{0,5000}",
    ) {
        let a = chunk_text(&s);
        let b = chunk_text(&s);
        prop_assert_eq!(
            a, b,
            "chunk_text must be deterministic for the same input"
        );
    }
}

// ── Content preservation ──────────────────────────────────────────────────────

proptest! {
    /// Reassembling chunks (first chunk in full, then non-overlapping suffix of
    /// each subsequent chunk) must reproduce the original text exactly.
    ///
    /// This property guarantees no content is dropped or duplicated.
    #[test]
    fn chunk_text_reassembly_reproduces_original(
        // Only test inputs longer than MAX to exercise the multi-chunk path.
        // Shorter inputs are already tested by the fast-path tests in input.rs.
        repeated_char in "[a-zA-Z]",
        count in (MAX_CHUNK + 1)..=(MAX_CHUNK * 4),
    ) {
        let s: String = repeated_char.repeat(count);
        let chunks = chunk_text(&s);

        prop_assume!(!chunks.is_empty());

        let mut reconstructed = chunks[0].clone();
        for chunk in chunks.iter().skip(1) {
            let novel: String = chunk.chars().skip(OVERLAP).collect();
            reconstructed.push_str(&novel);
        }

        prop_assert_eq!(
            reconstructed, s,
            "reassembled chunks must equal the original text"
        );
    }
}

// ── Chunk count monotonicity ──────────────────────────────────────────────────

proptest! {
    /// For purely ASCII text, chunk count must be at least 1 and must not exceed
    /// ceil(len / (MAX_CHUNK - OVERLAP)) + 1. This is a loose upper bound that
    /// guards against pathological chunk explosion.
    #[test]
    fn chunk_text_count_within_reasonable_bounds(
        s in "[a-z]{0,8000}",
    ) {
        let chunks = chunk_text(&s);
        let char_count = s.chars().count();
        let step = MAX_CHUNK - OVERLAP;
        // Conservative upper bound: worst-case every chunk is one overlap step wide.
        let upper = if char_count <= MAX_CHUNK {
            1
        } else {
            char_count.div_ceil(step) + 1
        };
        prop_assert!(
            !chunks.is_empty(),
            "must have at least one chunk for any input"
        );
        prop_assert!(
            chunks.len() <= upper,
            "chunk count {} exceeds upper bound {} for input len {}",
            chunks.len(), upper, char_count
        );
    }
}

// ── Unicode safety ───────────────────────────────────────────────────────────

proptest! {
    /// `chunk_text` must never produce a chunk that contains invalid UTF-8
    /// boundaries (i.e. every chunk must round-trip through `chars().collect()`).
    #[test]
    fn chunk_text_chunks_are_valid_unicode_strings(
        s in "\\PC{0,5000}",
    ) {
        for (i, chunk) in chunk_text(&s).iter().enumerate() {
            let round_trip: String = chunk.chars().collect();
            prop_assert!(
                *chunk == round_trip,
                "chunk {} failed UTF-8 round-trip — codepoint split detected",
                i
            );
        }
    }
}

proptest! {
    /// With multi-byte Unicode input (2-byte chars like 'é'), no chunk may exceed
    /// MAX_CHUNK characters (even though byte length may be up to 2×MAX_CHUNK).
    #[test]
    fn chunk_text_multibyte_no_chunk_exceeds_max_chars(
        // 2-byte chars: Latin Extended-A block U+0100–U+017F
        base_char in prop::char::range('\u{0100}', '\u{017F}'),
        count in 0usize..=6000,
    ) {
        let s: String = std::iter::repeat_n(base_char, count).collect();
        for (i, chunk) in chunk_text(&s).iter().enumerate() {
            let char_count = chunk.chars().count();
            prop_assert!(
                char_count <= MAX_CHUNK,
                "multi-byte chunk {i} has {char_count} chars, exceeding MAX={MAX_CHUNK}"
            );
        }
    }
}

// ── chunk_markdown property tests ────────────────────────────────────────────
//
// chunk_markdown uses MarkdownSplitter with 500–2000 char range and 200-char
// overlap. Its invariants differ from chunk_text:
//   1. Empty input → empty Vec (not vec![""] — the fast-path difference).
//   2. No empty/whitespace chunks — the retain() filter removes them.
// Control characters panic inside text-splitter's regex engine, so all
// chunk_markdown tests use markdown_safe_input() instead of unconstrained ".".

/// Safe input strategy: printable ASCII + `\n` + `\t`.
/// Excludes control characters (0x00–0x1F except space/newline/tab) that panic
/// inside the text-splitter markdown regex engine.
fn markdown_safe_input(max_len: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::char::ranges(
            vec![
                ' '..='~',   // printable ASCII (space through tilde)
                '\n'..='\n', // newlines (needed for markdown paragraphs)
                '\t'..='\t', // tabs
            ]
            .into(),
        ),
        0..=max_len,
    )
    .prop_map(|chars| chars.into_iter().collect::<String>())
}

proptest! {
    /// No chunk produced by `chunk_markdown` may exceed 2000 characters.
    #[test]
    fn chunk_markdown_no_chunk_exceeds_max_chars(
        s in markdown_safe_input(10000),
    ) {
        for (i, chunk) in chunk_markdown(&s).iter().enumerate() {
            let char_count = chunk.chars().count();
            prop_assert!(
                char_count <= MAX_CHUNK,
                "chunk_markdown chunk {i} has {char_count} chars, exceeding MAX={MAX_CHUNK}"
            );
        }
    }
}

proptest! {
    /// No chunk produced by `chunk_markdown` may be empty or whitespace-only.
    /// This property locks in the `retain(|c| !c.trim().is_empty())` guard.
    #[test]
    fn chunk_markdown_no_empty_or_whitespace_chunks(
        s in markdown_safe_input(10000),
    ) {
        for (i, chunk) in chunk_markdown(&s).iter().enumerate() {
            prop_assert!(
                !chunk.trim().is_empty(),
                "chunk_markdown chunk {i} is empty or whitespace-only"
            );
        }
    }
}

proptest! {
    /// `chunk_markdown` must be deterministic: same input → same output.
    #[test]
    fn chunk_markdown_is_deterministic(
        s in markdown_safe_input(5000),
    ) {
        let a = chunk_markdown(&s);
        let b = chunk_markdown(&s);
        prop_assert_eq!(
            a, b,
            "chunk_markdown must be deterministic for the same input"
        );
    }
}

proptest! {
    /// Non-empty, non-whitespace input must produce at least one chunk.
    /// (Empty input correctly returns an empty Vec — tested in input.rs.)
    #[test]
    fn chunk_markdown_non_whitespace_input_yields_at_least_one_chunk(
        s in "[a-zA-Z0-9]{1,10000}",
    ) {
        let chunks = chunk_markdown(&s);
        prop_assert!(
            !chunks.is_empty(),
            "non-whitespace input must produce at least one chunk"
        );
    }
}
