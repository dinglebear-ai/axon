# Error Handling Strategy

**Tracking issue:** A-M-08
**Status:** Documentation only — AxonError enum not yet implemented
**Last updated:** 2026-03-04

---

## Table of Contents

1. [Current State](#current-state)
2. [Problems](#problems)
3. [Proposed AxonError Enum](#proposed-axonerror-enum)
4. [MCP Error Code Mapping](#mcp-error-code-mapping)
5. [Migration Path](#migration-path)

---

## Current State

Command boundaries use `Box<dyn Error>`:

```rust
pub async fn run_scrape(cfg: &Config) -> Result<(), Box<dyn Error>>
pub async fn start_crawl_job(cfg: &Config, url: &str) -> Result<Uuid, Box<dyn Error>>
```

Internal helpers use typed errors in some places:

- `crates/core/http/error.rs` — `HttpError` enum (InvalidUrl, BlockedScheme, BlockedHost, BlockedIpRange)
- `sqlx::Error` — propagated as-is from database operations
- `lapin::Error` — propagated as-is from AMQP operations

Everything else uses `Box<dyn Error>` with `.map_err(|e| format!("message: {e}").into())` patterns, losing the error type information at command boundaries.

---

## Problems

### 1. MCP handlers cannot inspect error variants

The MCP server (`crates/mcp/`) needs to distinguish between:
- SSRF/security rejections → return a `403`-equivalent error code with a clear message
- Service-unreachable errors → return a retriable error code
- Invalid input → return an `invalid_params` error code
- General failures → return a generic error message

With `Box<dyn Error>`, the MCP handler must pattern-match on error string content, which is fragile and breaks when error messages change.

### 2. No structured retry decisions

Workers need to know whether a failed job should be retried:
- Network timeout → retry
- SSRF blocked URL → do NOT retry (permanent failure)
- Invalid content → do NOT retry
- DB connection lost → retry

Without typed errors at the job boundary, this decision is made by inspecting `.to_string()` output.

### 3. Diagnostic quality

`Box<dyn Error>` error messages are often opaque. When a user runs `axon doctor`, errors from nested `?` propagation produce messages like "invalid input syntax for type uuid: 'abc'" with no context about which operation triggered the error.

---

## Proposed AxonError Enum

```rust
// crates/core/error.rs  (NEW FILE — not yet created)

use std::fmt;

#[derive(Debug)]
pub enum AxonError {
    /// URL failed SSRF validation or was blocked by policy.
    /// Permanent — do not retry.
    SecurityRejection { url: String, reason: String },

    /// The URL is syntactically invalid.
    /// Permanent — do not retry.
    InvalidUrl(String),

    /// A required external service (Postgres, Redis, Qdrant, TEI, LLM) is unreachable.
    /// Retriable after backoff.
    ServiceUnavailable { service: &'static str, source: Box<dyn std::error::Error + Send + Sync> },

    /// A database operation failed.
    Database(sqlx::Error),

    /// An AMQP operation failed.
    Amqp(lapin::Error),

    /// HTTP request failed.
    Http { url: String, status: Option<u16>, source: Box<dyn std::error::Error + Send + Sync> },

    /// The crawled content is empty or below the minimum quality threshold.
    /// Permanent — do not retry with the same config.
    ThinContent { url: String, chars: usize, threshold: usize },

    /// A job with the given ID was not found.
    JobNotFound(uuid::Uuid),

    /// An operation was attempted on a job in an incompatible state.
    JobStateConflict { id: uuid::Uuid, current: String, expected: String },

    /// The LLM endpoint returned an error.
    LlmError { model: String, source: Box<dyn std::error::Error + Send + Sync> },

    /// General I/O error (file system operations).
    Io(std::io::Error),

    /// A catch-all for errors that have not yet been migrated to typed variants.
    /// TODO: remove all uses of this variant as migration proceeds.
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl AxonError {
    /// Returns true if this error represents a condition that may resolve on retry.
    pub fn is_retriable(&self) -> bool {
        matches!(
            self,
            Self::ServiceUnavailable { .. } | Self::Amqp(_) | Self::Http { status: None, .. }
        )
    }

    /// Returns true if this error is a security policy rejection.
    pub fn is_security_rejection(&self) -> bool {
        matches!(self, Self::SecurityRejection { .. } | Self::InvalidUrl(_))
    }
}
```

---

## MCP Error Code Mapping

When the MCP server catches an `AxonError`, it should map it to the appropriate MCP error code:

| AxonError variant | MCP error code | Notes |
|-------------------|---------------|-------|
| `SecurityRejection` | `invalid_params` | The URL was rejected by policy — client error |
| `InvalidUrl` | `invalid_params` | Malformed URL — client error |
| `ServiceUnavailable` | Internal error (retriable) | Signal the client to retry |
| `Database` | Internal error | Log with context, return generic message |
| `JobNotFound` | `invalid_params` | The job ID does not exist |
| `JobStateConflict` | `invalid_params` | Wrong job state for requested operation |
| `ThinContent` | Soft warning in result | Not an error — include in result payload |
| `LlmError` | Internal error | Include model name in log, generic message to client |
| `Other` | Internal error | Opaque — log full chain, generic message to client |

### Pattern (MCP handler pseudocode)

```rust
match run_crawl(cfg, url).await {
    Ok(result) => build_success_response(result),
    Err(AxonError::SecurityRejection { reason, .. }) => {
        mcp_error(McpErrorCode::InvalidParams, format!("URL blocked: {reason}"))
    }
    Err(AxonError::InvalidUrl(msg)) => {
        mcp_error(McpErrorCode::InvalidParams, msg)
    }
    Err(AxonError::ServiceUnavailable { service, .. }) => {
        log::warn!("service {} unavailable", service);
        mcp_error(McpErrorCode::InternalError, "service temporarily unavailable — retry")
    }
    Err(e) => {
        log::error!("unexpected error: {e:?}");
        mcp_error(McpErrorCode::InternalError, "internal error")
    }
}
```

---

## Migration Path

### Phase 1: Create `crates/core/error.rs`

Define the `AxonError` enum. Implement `std::error::Error`, `fmt::Display`, `From<sqlx::Error>`, `From<lapin::Error>`, `From<std::io::Error>`.

Add `pub use crate::crates::core::error::AxonError;` to `crates/core/config.rs` (or a dedicated re-export).

### Phase 2: Migrate `HttpError` → `AxonError`

`HttpError` in `crates/core/http/error.rs` maps cleanly to `AxonError::SecurityRejection` and `AxonError::InvalidUrl`. Migrate and remove `HttpError`.

### Phase 3: Migrate job command boundaries

Change `Result<_, Box<dyn Error>>` → `Result<_, AxonError>` at command handler level. This is the highest-impact change — all `?` propagations must be reviewed to ensure they produce the right variant.

Start with:
- `crates/cli/commands/scrape.rs` (small, self-contained)
- `crates/cli/commands/map.rs` (small, self-contained)

Then move to larger commands (crawl, ask, embed).

### Phase 4: Update MCP handlers

Once command handlers return `AxonError`, update MCP handlers to match on variants instead of string inspection.

### Phase 5: Update worker retry logic

Replace string-based retry heuristics in worker loops with `error.is_retriable()`.

### Phase 6: Remove `AxonError::Other`

The `Other` variant is a migration escape hatch. Once all call sites use typed variants, `Other` can be removed and the compiler enforces exhaustive error handling.
