package com.axon.app.data.repository

import com.axon.app.core.api.SiteSourceRequest
import com.axon.app.core.api.models.SourceRequest

data class SiteSourceSubmitOptions(
    val maxPages: Int? = null,
    val maxDepth: Int? = null,
    val renderMode: String? = null,
    val includeSubdomains: Boolean? = null,
) {
    fun requestFor(url: String): SiteSourceRequest =
        SiteSourceRequest(
            urls = listOf(url),
            maxPages = maxPages,
            maxDepth = maxDepth,
            renderMode = renderMode,
            includeSubdomains = includeSubdomains,
        )
}

data class SourceSubmitOptions(
    val embed: Boolean? = null,
    val collection: String? = null,
) {
    fun requestFor(target: String): SourceRequest =
        SourceRequest(
            source = target,
            embed = embed,
            collection = collection,
        )
}
