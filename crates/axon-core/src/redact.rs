//! Shared secret redaction (S-L1 unification).
//!
//! Single regex-based redactor used everywhere untrusted text is scrubbed
//! before it is logged or surfaced to a caller — Gemini subprocess stderr
//! tails (`core::llm::headless`), OpenAI-compat error bodies
//! (`core::llm::openai_compat`), and any future call site.
//!
//! Unlike the per-call-site implementations it replaces, this operates on the
//! **entire string** rather than whitespace-delimited tokens, so secrets with
//! no surrounding whitespace (e.g. `Authorization:Bearer AIza...`) are still
//! caught. It is a superset of every redactor it replaces. It matches known key
//! shapes — Google API keys (`AIza...`), Google OAuth tokens (`ya29.<token>`),
//! OpenAI keys (`sk-...`), GitHub tokens (`ghp_`/`gho_`/`ghu_`/`ghs_`/`ghr_`),
//! `atk_` tokens — plus `Authorization:`/`Authorization=` header values and the
//! `API_KEY`/`TOKEN`/`SECRET` (`=` or `:`) marker rules, plus a high-entropy 32+
//! char run.
//!
//! The token-anchored prefix rules (`sk-`, `gh*_`, `atk_`) use a `\b` word
//! boundary so they fire only at the start of a token — `task-force` is not
//! redacted by the `sk-` rule, but ` sk-...` is. They match any length, so a
//! short/malformed token in an error tail is still caught.
//!
//! The high-entropy fallback is gated on a Shannon-entropy threshold so that
//! degenerate runs (long benign padding like `xxxxxxxx…`, repeated filler) are
//! left intact while genuinely random-looking tokens are redacted.
//!
//! This module also owns sensitive-*name* detection ([`is_secret_like`], for
//! header/field/file names) in addition to redacting secret *values* embedded
//! in free text — a single home for both halves of the S-L1 policy.

use regex::Regex;
use std::sync::LazyLock;

/// Placeholder substituted for every matched secret span.
pub const REDACTION_PLACEHOLDER: &str = "[REDACTED]";

/// Minimum Shannon entropy (bits/char) for the high-entropy fallback to fire.
/// Repeated/low-diversity runs fall below this and are left untouched; real
/// API keys and tokens sit comfortably above it.
///
/// Private (module-private, not `pub`), but still visible to the child
/// `redact::detectors` module — Rust privacy is scoped to the defining
/// module *and its descendants*. This lets the structured detector set
/// reuse the same threshold and entropy math for its Gitea/GitLab/OAuth
/// opaque-token classifier instead of re-implementing entropy scoring.
const MIN_ENTROPY_BITS: f64 = 3.0;

/// Structured secret shapes — redacted unconditionally.
static STRUCTURED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
          AIza[0-9A-Za-z_\-]{35}              # Google API key
        | ya29\.[\w\-]+                        # Google OAuth access token
        | \bsk-[A-Za-z0-9][A-Za-z0-9_\-]*      # OpenAI-style key (token-anchored, any length)
        | \bgh[pousr]_[A-Za-z0-9]+             # GitHub token ghp_/gho_/ghu_/ghs_/ghr_ (any length, no tail leak)
        | \batk_[A-Za-z0-9_\-]+                # atk_-prefixed token
        | (?i:authorization)[:=][\ \t]*(?:(?i:bearer|basic|token)[\ \t]+)?\S*  # Authorization value, with or without scheme word/spacing (horizontal space only so a bare header never swallows the next line)
        | \S*(?i:API_KEY|TOKEN|SECRET)[:=]\S*  # any non-ws run with API_KEY/TOKEN/SECRET followed by = or : (case-insensitive)
        ",
    )
    .expect("structured secret regex is valid")
});

/// Candidate runs for the entropy-gated fallback.
static HIGH_ENTROPY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z0-9_\-]{32,}").expect("high-entropy regex is valid"));

/// Replace every secret-looking span in `text` with [`REDACTION_PLACEHOLDER`].
///
/// Safe to call on arbitrary untrusted text; non-secret content is returned
/// unchanged.
#[must_use]
pub fn redact_secrets(text: &str) -> String {
    let structured = STRUCTURED_RE.replace_all(text, REDACTION_PLACEHOLDER);
    HIGH_ENTROPY_RE
        .replace_all(&structured, |caps: &regex::Captures| {
            let run = &caps[0];
            if shannon_entropy_bits(run) >= MIN_ENTROPY_BITS {
                REDACTION_PLACEHOLDER.to_string()
            } else {
                run.to_string()
            }
        })
        .into_owned()
}

/// Shannon entropy of `s` in bits per character. Candidate runs are ASCII
/// (`[A-Za-z0-9_-]`), so byte-frequency counting is exact.
fn shannon_entropy_bits(s: &str) -> f64 {
    let mut counts = [0u32; 256];
    let mut total = 0u32;
    for b in s.bytes() {
        counts[b as usize] += 1;
        total += 1;
    }
    if total == 0 {
        return 0.0;
    }
    let total_f = f64::from(total);
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = f64::from(c) / total_f;
            -p * p.log2()
        })
        .sum()
}

/// Returns `true` when `lower_name` (already lowercased by caller) looks like
/// a secret key, credential file, or sensitive header/field name. Single source
/// of truth for both embed path validation and error-body redaction.
pub fn is_secret_like(lower_name: &str) -> bool {
    // Private-key filenames
    if lower_name == "id_rsa"
        || lower_name == "id_dsa"
        || lower_name == "id_ecdsa"
        || lower_name == "id_ed25519"
    {
        return true;
    }
    // Extensions that commonly hold key material
    if lower_name.ends_with(".pem") || lower_name.ends_with(".key") {
        return true;
    }
    // Semantic keywords
    if lower_name.contains("secret")
        || lower_name.contains("credential")
        || lower_name.contains("password")
    {
        return true;
    }
    // Token / API key patterns
    if lower_name.contains("api_key")
        || lower_name.contains("apikey")
        || lower_name == "authorization"
        || lower_name == "proxy-authorization"
        || lower_name == "access_token"
        || lower_name == "refresh_token"
        || lower_name == "id_token"
        || lower_name.ends_with("_token")
        || lower_name.contains("token")
    {
        return true;
    }
    false
}

mod boundary;
mod detectors;

pub use boundary::{
    DefaultRedactor, MAX_REDACTABLE_TEXT_BYTES, REDACTION_VERSION, RedactionContext,
    RedactionReport, RedactionStatus, RedactionSurface, Redactor, redact_metadata,
    redact_metadata_checked, redact_public_write, redact_text_checked, stamp_redaction_metadata,
};
pub use detectors::{
    BARE_SECRET_TOKEN_PREFIXES, FORBIDDEN_FIELD_FRAGMENTS, FORBIDDEN_VALUE_FRAGMENTS,
    SECRET_LIKE_FIELD_FRAGMENTS, contains_bare_secret_token, forbidden_field_name,
    raw_dotenv_assignment, secret_like_field_name, value_contains_secret,
    value_is_absolute_local_path,
};

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
