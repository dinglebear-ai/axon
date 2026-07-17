package com.axon.app.core.api

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject

// ── Requests ──────────────────────────────────────────────────────────────────

/** Request body for POST /v1/ask and POST /v1/ask/stream. Mirrors `RestAskRequest`. */
@Serializable
data class AskRequest(
    val query: String,
    val collection: String? = null,
    val diagnostics: Boolean? = null,
    val explain: Boolean? = null,
    @SerialName("hybrid_search") val hybridSearch: Boolean? = null,
    @SerialName("ask_chunk_limit") val chunkLimit: Int? = null,
    @SerialName("ask_full_docs") val fullDocs: Int? = null,
    @SerialName("ask_max_context_chars") val maxContextChars: Int? = null,
    @SerialName("ask_hybrid_candidates") val hybridCandidates: Int? = null,
)

/** Request body for POST /v1/chat and POST /v1/chat/stream. Direct LLM, no RAG fields. */
@Serializable
data class ChatRequest(
    val message: String,
)

/** Response body from POST /v1/chat. */
@Serializable
data class ChatResponse(
    val message: String,
    val answer: String,
    val model: String? = null,
)

/** Request body for POST /v1/query. */
@Serializable
data class QueryRequest(
    val query: String,
    val limit: Int = 10,
    val offset: Int? = null,
    val collection: String? = null,
    val since: String? = null,
    val before: String? = null,
    @SerialName("hybrid_search") val hybridSearch: Boolean? = null,
)

/** Query parameters for GET /v1/sources. */
@Serializable
data class SourcesRequest(
    val limit: Int = 50,
    val offset: Int = 0,
    val domain: String? = null,
    val cursor: String? = null,
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

// ── Retrieve ──────────────────────────────────────────────────────────────────

/** Request body for POST /v1/retrieve — fetch the full stored document for a URL. */
@Serializable
data class RetrieveRequest(
    val url: String,
    val collection: String? = null,
    val since: String? = null,
    val before: String? = null,
    @SerialName("max_points") val maxPoints: Int? = null,
    val cursor: String? = null,
    @SerialName("token_budget") val tokenBudget: Int? = null,
)

/** Response body from POST /v1/retrieve. Mirrors the server-side `RetrieveResult`. */
@Serializable
data class RetrieveResponse(
    @SerialName("chunk_count") val chunkCount: Int = 0,
    val content: String = "",
    @SerialName("requested_url") val requestedUrl: String? = null,
    @SerialName("matched_url") val matchedUrl: String? = null,
    val truncated: Boolean = false,
    val warnings: List<String> = emptyList(),
    @SerialName("token_estimate") val tokenEstimate: Int? = null,
    @SerialName("next_cursor") val nextCursor: String? = null,
    @SerialName("remaining_tokens_estimate") val remainingTokensEstimate: Int? = null,
    @SerialName("refresh_status") val refreshStatus: String? = null,
)

// ── Stats ─────────────────────────────────────────────────────────────────────

/** Response body from GET /v1/stats. */
@Serializable
data class StatsResponse(
    val payload: JsonObject,
)

// ── Scrape ────────────────────────────────────────────────────────────────────

/** One-page convenience request projected onto POST /v1/sources. */
@Serializable
data class ScrapeRequest(
    val url: String,
    val embed: Boolean? = null,
    @SerialName("render_mode") val renderMode: String? = null, // "http"|"chrome"|"auto_switch"
    val format: String? = null, // "markdown"|"html"|"rawHtml"|"json"
    val collection: String? = null,
)

/** One-page content projection from the canonical source result. */
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
    val offset: Int? = null,
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

/** Request body for POST /v1/research. Mirrors `RestResearchRequest`. */
@Serializable
data class ResearchRequest(
    val query: String,
    val limit: Int? = null,
    val offset: Int? = null,
    @SerialName("time_range") val timeRange: String? = null,
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
 * `parseStreamEvent` (AxonClientStreaming.kt) rather than via [kotlinx.serialization]
 * because the discriminator field name (`"type"`) conflicts with Kotlin's type keyword
 * and the sealed interface hierarchy needs no serialization annotations for the
 * streaming path.
 */
sealed interface AskStreamEvent {
    /** Phase indicator — emitted before synthesis starts (e.g. "retrieval", "synthesis"). */
    data class Meta(
        val phase: String,
    ) : AskStreamEvent

    /** Incremental answer token from the LLM. */
    data class Delta(
        val text: String,
    ) : AskStreamEvent

    /** Synthesis complete — [answer] is the full assembled answer. */
    data class Done(
        val answer: String,
    ) : AskStreamEvent

    /** Server-side or network error during streaming. */
    data class Error(
        val message: String,
    ) : AskStreamEvent
}

// ── Site source ───────────────────────────────────────────────────────────────

/** Site-scoped source request submitted through POST /v1/sources. */
@Serializable
data class SiteSourceRequest(
    val urls: List<String>,
    @SerialName("max_pages") val maxPages: Int? = null,
    @SerialName("max_depth") val maxDepth: Int? = null,
    @SerialName("render_mode") val renderMode: String? = null, // "http"|"chrome"|"auto-switch"
    @SerialName("include_subdomains") val includeSubdomains: Boolean? = null,
    val embed: Boolean? = null,
    val collection: String? = null,
    val headers: List<String> = emptyList(),
)

/** Response body from POST /v1/sources (source job submission). */
@Serializable
data class SourceJobResponse(
    @SerialName("job_id") val jobId: String = "",
    val url: String = "",
)
