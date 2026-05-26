package com.axon.app.data.remote

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject

// ── Requests ──────────────────────────────────────────────────────────────────

@Serializable
data class AskRequest(
    val query: String,
    val collection: String? = null,
)

@Serializable
data class QueryRequest(
    val query: String,
    val limit: Int = 10,
    val collection: String? = null,
)

@Serializable
data class SourcesRequest(
    val limit: Int = 50,
    val offset: Int = 0,
    val collection: String? = null,
)

// ── Ask response ──────────────────────────────────────────────────────────────

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

@Serializable
data class QueryResponse(
    val results: List<QueryHit>,
)

@Serializable
data class QueryHit(
    val rank: Long,
    val score: Double,
    @SerialName("rerank_score") val rerankScore: Double = 0.0,
    val url: String,
    val source: String,
    val snippet: String,
    @SerialName("chunk_index") val chunkIndex: Long? = null,
)

// ── Sources response ──────────────────────────────────────────────────────────
// Rust serializes Vec<(String, usize)> as [[url, count], ...].
// We keep the raw JsonArray and let AxonRepository map it.

@Serializable
data class SourcesResponse(
    val count: Int,
    val limit: Int,
    val offset: Int,
    val urls: JsonArray,
)

// ── Stats ─────────────────────────────────────────────────────────────────────

@Serializable
data class StatsResponse(
    val payload: JsonObject,
)

// ── Scrape ────────────────────────────────────────────────────────────────────

@Serializable
data class ScrapeRequest(
    val url: String,
    val embed: Boolean? = null,
    val collection: String? = null,
)

@Serializable
data class ScrapeResponse(
    val url: String,
    val markdown: String,
    val output: String = "",
)

// ── Map ───────────────────────────────────────────────────────────────────────

@Serializable
data class MapRequest(
    val url: String,
    val limit: Int? = null,
)

@Serializable
data class MapResponse(
    val url: String,
    @SerialName("mapped_urls") val mappedUrls: Long,
    val total: Long,
    val urls: List<String>,
)

// ── Research ──────────────────────────────────────────────────────────────────

@Serializable
data class ResearchRequest(
    val query: String,
    val limit: Int? = null,
)

@Serializable
data class ResearchResponse(
    val payload: ResearchPayload,
)

@Serializable
data class ResearchPayload(
    val query: String,
    val search_results: List<ResearchHit> = emptyList(),
    val summary: String? = null,
)

@Serializable
data class ResearchHit(
    val position: Int,
    val title: String,
    val url: String,
    val snippet: String? = null,
)

// ── Crawl ─────────────────────────────────────────────────────────────────────

@Serializable
data class CrawlRequest(
    val urls: List<String>,
    val max_pages: Int? = null,
    val max_depth: Int? = null,
    val collection: String? = null,
)

@Serializable
data class CrawlJobResponse(
    val job_id: String,
    val url: String = "",
)

@Serializable
data class CrawlStatusResponse(
    val job_id: String,
    val status: String,
    val url: String = "",
    val pages_crawled: Int? = null,
    val error: String? = null,
)
