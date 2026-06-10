package com.axon.app.data.repository

/**
 * Domain-layer job type. UI code uses [JobFamily]; wire routing details
 * ([AxonClient.JobKind] and its [path] field) stay inside [AxonRepository].
 */
enum class JobFamily {
    Crawl, Embed, Extract, Ingest;

    fun label(): String = when (this) {
        Crawl -> "Crawl"
        Embed -> "Embed"
        Extract -> "Extract"
        Ingest -> "Ingest"
    }

    fun drillTitle(): String = when (this) {
        Crawl -> "Crawls"
        Embed -> "Embeddings"
        Extract -> "Extractions"
        Ingest -> "Ingestions"
    }
}
