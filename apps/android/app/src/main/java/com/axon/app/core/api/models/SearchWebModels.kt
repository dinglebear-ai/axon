package com.axon.app.core.api.models

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** POST /v1/search request — Tavily web search; mirrors `RestSearchRequest`. */
@Serializable
data class SearchWebRequest(
    val query: String,
    val limit: Int? = null,
    val offset: Int? = null,
    @SerialName("time_range") val timeRange: String? = null,       // "day"|"week"|"month"|null
)

@Serializable
data class SearchWebHit(
    val title: String = "",
    val url: String = "",
    val snippet: String? = null,
    val score: Double? = null,
)

@Serializable
data class SourceJobRef(
    @SerialName("job_id") val jobId: String,
    val url: String,
)

@Serializable
data class SourceJobRejection(
    val url: String? = null,
    val reason: String = "",
)

@Serializable
data class SearchWebResponse(
    val query: String = "",
    val results: List<SearchWebHit> = emptyList(),
    @SerialName("source_index_status") val sourceIndexStatus: String = "not_queued",
    @SerialName("source_jobs") val sourceJobs: List<SourceJobRef> = emptyList(),
    @SerialName("source_jobs_rejected") val sourceJobsRejected: List<SourceJobRejection> = emptyList(),
)
