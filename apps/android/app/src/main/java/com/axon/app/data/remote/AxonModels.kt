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
