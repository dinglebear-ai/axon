package com.axon.app.data.remote.models

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
data class CrawlJobRef(
    @SerialName("job_id") val jobId: String,
    val url: String,
)

@Serializable
data class AutoCrawlStatus(
    val enqueued: Int = 0,
    val skipped: Int = 0,
)

@Serializable
data class SearchWebResponse(
    val query: String = "",
    val results: List<SearchWebHit> = emptyList(),
    @SerialName("auto_crawl_status") val autoCrawlStatus: AutoCrawlStatus? = null,
    @SerialName("crawl_jobs") val crawlJobs: List<CrawlJobRef> = emptyList(),
)
