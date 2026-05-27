package com.axon.app.data.remote

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject

// ── Requests ──────────────────────────────────────────────────────────────────

/** Request body for POST /v1/ask and POST /v1/ask/stream. */
@Serializable
data class AskRequest(
    val query: String,
    val collection: String? = null,
)

/** Request body for POST /v1/query. */
@Serializable
data class QueryRequest(
    val query: String,
    val limit: Int = 10,
    val collection: String? = null,
)

/** Query parameters for GET /v1/sources. */
@Serializable
data class SourcesRequest(
    val limit: Int = 50,
    val offset: Int = 0,
    val collection: String? = null,
)

// ── Ask response ──────────────────────────────────────────────────────────────

/** Response body from POST /v1/ask (non-streaming). */
@Serializable
data class AskResponse(
    val query: String,
    val answer: String,
    @SerialName("timing_ms") val timingMs: AskTiming? = null,
)

@Serializable
data class AskTiming(
    @SerialName("total_ms") val totalMs: Long? = null,
)

// ── Query response ────────────────────────────────────────────────────────────

/** Response body from POST /v1/query. */
@Serializable
data class QueryResponse(
    val results: List<QueryHit>,
)

@Serializable
data class QueryHit(
    val rank: Long = 0L,
    val score: Double = 0.0,
    @SerialName("rerank_score") val rerankScore: Double = 0.0,
    val url: String = "",
    val source: String = "",
    val snippet: String = "",
    @SerialName("chunk_index") val chunkIndex: Long? = null,
)

// ── Sources response ──────────────────────────────────────────────────────────

/**
 * Response body from GET /v1/sources.
 *
 * The server serialises `Vec<(String, usize)>` as a JSON array of two-element arrays:
 * `[[url, chunkCount], ...]`. The raw [JsonArray] is kept here so [AxonRepository] can
 * perform the structural mapping with full control over error handling.
 */
@Serializable
data class SourcesResponse(
    val count: Int,
    val limit: Int,
    val offset: Int,
    val urls: JsonArray,
)

// ── Stats ─────────────────────────────────────────────────────────────────────

/** Response body from GET /v1/stats. */
@Serializable
data class StatsResponse(
    val payload: JsonObject,
)

// ── Scrape ────────────────────────────────────────────────────────────────────

/** Request body for POST /v1/scrape. */
@Serializable
data class ScrapeRequest(
    val url: String,
    val embed: Boolean? = null,
)

/** Response body from POST /v1/scrape. */
@Serializable
data class ScrapeResponse(
    val url: String = "",
    val markdown: String = "",
    val output: String = "",
)

// ── Map ───────────────────────────────────────────────────────────────────────

/** Request body for POST /v1/map. */
@Serializable
data class MapRequest(
    val url: String,
    val limit: Int? = null,
)

/** Response body from POST /v1/map. */
@Serializable
data class MapResponse(
    val url: String = "",
    @SerialName("mapped_urls") val mappedUrls: Long = 0L,
    val total: Long = 0L,
    val urls: List<String> = emptyList(),
)

// ── Research ──────────────────────────────────────────────────────────────────

/** Request body for POST /v1/research. */
@Serializable
data class ResearchRequest(
    val query: String,
    val limit: Int? = null,
)

/** Response body from POST /v1/research. */
@Serializable
data class ResearchResponse(
    val payload: ResearchPayload,
)

@Serializable
data class ResearchPayload(
    val query: String,
    @SerialName("search_results") val searchResults: List<ResearchHit> = emptyList(),
    val summary: String? = null,
)

@Serializable
data class ResearchHit(
    val position: Int,
    val title: String,
    val url: String,
    val snippet: String? = null,
)

// ── Ask stream events ─────────────────────────────────────────────────────────

/**
 * Discriminated union of SSE events emitted by POST /v1/ask/stream.
 *
 * Each event is a JSON object with a `"type"` field. Parsing is done manually in
 * [AxonClient.parseStreamEvent] rather than via [kotlinx.serialization] because the
 * discriminator field name (`"type"`) conflicts with Kotlin's type keyword and the
 * sealed interface hierarchy needs no serialization annotations for the streaming path.
 */
sealed interface AskStreamEvent {
    /** Phase indicator — emitted before synthesis starts (e.g. "retrieval", "synthesis"). */
    data class Meta(val phase: String) : AskStreamEvent
    /** Incremental answer token from the LLM. */
    data class Delta(val text: String) : AskStreamEvent
    /** Synthesis complete — [answer] is the full assembled answer. */
    data class Done(val answer: String) : AskStreamEvent
    /** Server-side or network error during streaming. */
    data class Error(val message: String) : AskStreamEvent
}

// ── Crawl ─────────────────────────────────────────────────────────────────────

/** Request body for POST /v1/crawl. */
@Serializable
data class CrawlRequest(
    val urls: List<String>,
    @SerialName("max_pages") val maxPages: Int? = null,
    @SerialName("max_depth") val maxDepth: Int? = null,
)

/** Response body from POST /v1/crawl (job submission). */
@Serializable
data class CrawlJobResponse(
    @SerialName("job_id") val jobId: String = "",
    val url: String = "",
)

/**
 * Top-level envelope from GET /v1/crawl/{job_id}.
 *
 * The server wraps the job detail in a `{"job": {...}}` envelope — this class is the
 * deserialisation target. [AxonClient.crawlStatus] extracts [job] and returns
 * [CrawlStatusResponse] directly so callers are not coupled to the envelope shape.
 */
@Serializable
data class CrawlStatusWrapper(
    val job: CrawlStatusResponse,
)

/** `result_json` sub-object inside [CrawlStatusResponse]. */
@Serializable
data class CrawlResultJson(
    @SerialName("pages_crawled") val pagesCrawled: Int? = null,
)

/**
 * Inner job detail returned by GET /v1/crawl/{job_id} (inside the [CrawlStatusWrapper] envelope).
 *
 * Key differences from the previously assumed flat shape:
 * - Primary key is `id`, not `job_id` — exposed via [jobId] for backward-compatible access.
 * - Error text is `error_text`, not `error`.
 * - Page count lives inside a nested `result_json` object — exposed via [pagesCrawled].
 */
@Serializable
data class CrawlStatusResponse(
    /** Server-assigned job UUID. Aliased as [jobId] for backward-compatible repository access. */
    val id: String = "",
    val status: String = "",
    val url: String = "",
    @SerialName("error_text") val error: String? = null,
    @SerialName("result_json") val resultJson: CrawlResultJson? = null,
) {
    /** Convenience alias for [id], matching the original flat-shape field name. */
    val jobId: String get() = id

    /** Pages crawled, nested inside [resultJson]. Null when the job has not completed. */
    val pagesCrawled: Int? get() = resultJson?.pagesCrawled
}
