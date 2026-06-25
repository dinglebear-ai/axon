package com.axon.app.data.repository

import com.axon.app.data.remote.CrawlRequest
import com.axon.app.data.remote.models.IngestRequest

data class CrawlSubmitOptions(
    val maxPages: Int? = null,
    val maxDepth: Int? = null,
    val renderMode: String? = null,
    val includeSubdomains: Boolean? = null,
) {
    fun requestFor(url: String): CrawlRequest = CrawlRequest(
        urls = listOf(url),
        maxPages = maxPages,
        maxDepth = maxDepth,
        renderMode = renderMode,
        includeSubdomains = includeSubdomains,
    )
}

data class IngestSubmitOptions(
    val includeSource: Boolean? = null,
) {
    fun requestFor(sourceType: String, target: String): IngestRequest = IngestRequest(
        sourceType = sourceType,
        target = target,
        includeSource = includeSource,
    )
}
