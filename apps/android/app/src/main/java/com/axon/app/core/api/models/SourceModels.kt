package com.axon.app.core.api.models

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Request body for POST /v1/sources — the unified source-pipeline entrypoint
 * that replaced the legacy `/v1/scrape`, `/v1/crawl`, `/v1/embed`, and
 * `/v1/ingest` routes (all four now hard-404; see `rest_tests.rs`).
 * Mirrors `axon_api::source::SourceRequest`. Only the subset of fields the
 * Android client currently populates is modeled here — the server fills the
 * rest via serde defaults (`#[serde(deny_unknown_fields)]` means unmodeled
 * fields must stay absent rather than sent as `null`).
 */
@Serializable
data class SourceRequest(
    val source: String,
    val embed: Boolean? = null,
    val collection: String? = null,
    val limits: SourceRequestLimits? = null,
)

/** Subset of `axon_api::source::SourceLimits` used for crawl page/depth caps. */
@Serializable
data class SourceRequestLimits(
    @SerialName("max_pages") val maxPages: Long? = null,
    @SerialName("max_depth") val maxDepth: Int? = null,
)

/**
 * Response body from POST /v1/sources. Mirrors `axon_api::source::SourceResult`.
 * Only fields needed to reconstruct the legacy scrape/crawl/embed/ingest UI
 * shapes are modeled; the full DTO also carries ledger/graph/counts/warnings
 * not yet surfaced by these call sites.
 */
@Serializable
data class SourceResult(
    @SerialName("job_id") val jobId: String = "",
    @SerialName("canonical_uri") val canonicalUri: String = "",
    val status: String = "",
    val inline: SourceInlineResult? = null,
    val job: SourceJobDescriptor? = null,
)

/** `axon_api::source::InlineSourceResult` — populated for synchronous, small results. */
@Serializable
data class SourceInlineResult(
    val content: SourceContentRef? = null,
)

/**
 * `axon_api::source::ContentRef` is a `kind`-tagged union
 * (`inline_text`/`inline_bytes`/`artifact`/`external`); Android only renders
 * the `inline_text` variant's `text` field today.
 */
@Serializable
data class SourceContentRef(
    val kind: String? = null,
    val text: String? = null,
)

/**
 * `axon_api::source::JobDescriptor` — present when indexing continues as a
 * background job. Only `id` and `status_url` are wire-serialized (the DTO's
 * `status`/`job_id`/timestamps fields are server-internal `#[serde(skip)]`).
 */
@Serializable
data class SourceJobDescriptor(
    val id: String = "",
    @SerialName("status_url") val statusUrl: String? = null,
)
