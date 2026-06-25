use std::fmt;

/// Wrapper for secret values that prevents accidental logging.
///
/// Implements `Debug` and `Display` as `[REDACTED]` so secrets never appear in
/// log output, panic messages, or formatted output. Access the inner value
/// explicitly with `.expose()` only where the raw value is required.
///
/// # Example
///
/// ```rust,ignore
/// use crate::config::secret::Secret;
///
/// let key = Secret::new("sk-supersecret".to_string());
/// assert_eq!(format!("{key:?}"), "[REDACTED]");
/// assert_eq!(key.expose(), "sk-supersecret");
/// ```
///
/// NOTE: Config fields do not yet use `Secret<T>` — migration is tracked in
/// docs/config-decomposition-plan.md (A-M-07). When ready, wrap fields and
/// update all call sites that access the raw value to use `.expose()`.
#[derive(Clone, Default)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    /// Wrap a value as a secret.
    pub fn new(val: T) -> Self {
        Self(val)
    }

    /// Intentionally expose the inner secret value.
    ///
    /// This is a deliberate access point — search for `.expose()` to audit
    /// all places where secret values are consumed.
    pub fn expose(&self) -> &T {
        &self.0
    }

    /// Consume the wrapper and return the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl<T: AsRef<str>> Secret<T> {
    /// Borrow the inner value as a `&str` without taking ownership.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl Secret<String> {
    /// Constant-time equality check for authentication comparisons.
    ///
    /// Use this instead of `==` whenever comparing against an expected secret
    /// (e.g. an API token from an incoming request). Unlike `String::eq`, this
    /// method does not short-circuit on the first differing byte, preventing
    /// timing-based side-channel attacks that could leak prefix length.
    ///
    /// Returns `true` if and only if the inner value equals `other` byte-for-byte.
    pub fn constant_time_eq(&self, other: &str) -> bool {
        let a = self.0.as_bytes();
        let b = other.as_bytes();
        if a.len() != b.len() {
            return false;
        }
        // XOR-fold: accumulates any differing bits without branching on content.
        a.iter()
            .zip(b.iter())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
    }
}

/// `PartialEq` uses `String::eq`, which short-circuits on the first mismatch.
///
/// **Do not use `==` for authentication comparisons** — it leaks prefix length
/// via timing. Use [`Secret::constant_time_eq`] instead for any auth check.
///
/// This impl is provided only for non-auth equality (e.g. struct derives,
/// config diffing) where constant-time guarantees are not required.
impl<T: PartialEq> PartialEq for Secret<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: Eq> Eq for Secret<T> {}
#[cfg(test)]
#[path = "secret_tests.rs"]
mod tests;
