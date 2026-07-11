package com.axon.app.data.repository

import com.axon.app.core.api.AskRequest
import com.axon.app.core.api.CrawlRequest
import com.axon.app.core.api.MapRequest
import com.axon.app.core.api.QueryRequest
import com.axon.app.core.api.ResearchRequest
import com.axon.app.core.api.ScrapeRequest
import com.axon.app.core.api.models.IngestRequest
import com.axon.app.core.api.models.SearchWebRequest
import com.axon.app.core.api.models.SummarizeRequest

/**
 * One method per wire DTO that has user-configurable mode-options.
 *
 * Implementations read persisted overrides from [ModeOptionsRepository] and merge them
 * into the request — [AxonRepository] stays ignorant of which fields exist per mode.
 *
 * The repository decorator pattern: every public AxonRepository call routes the request
 * through `applicator.apply(req)` before passing it to AxonClient.
 */
interface ModeOptionsApplicator {
    suspend fun apply(req: AskRequest): AskRequest
    suspend fun apply(req: QueryRequest): QueryRequest
    suspend fun apply(req: SummarizeRequest): SummarizeRequest
    suspend fun apply(req: ResearchRequest): ResearchRequest
    suspend fun apply(req: ScrapeRequest): ScrapeRequest
    suspend fun apply(req: CrawlRequest): CrawlRequest
    suspend fun apply(req: MapRequest): MapRequest
    suspend fun apply(req: SearchWebRequest): SearchWebRequest
    suspend fun apply(req: IngestRequest): IngestRequest
}

/** No-op applicator. Useful for tests and for AxonRepository defaults in test fixtures. */
object NoopModeOptionsApplicator : ModeOptionsApplicator {
    override suspend fun apply(req: AskRequest): AskRequest = req
    override suspend fun apply(req: QueryRequest): QueryRequest = req
    override suspend fun apply(req: SummarizeRequest): SummarizeRequest = req
    override suspend fun apply(req: ResearchRequest): ResearchRequest = req
    override suspend fun apply(req: ScrapeRequest): ScrapeRequest = req
    override suspend fun apply(req: CrawlRequest): CrawlRequest = req
    override suspend fun apply(req: MapRequest): MapRequest = req
    override suspend fun apply(req: SearchWebRequest): SearchWebRequest = req
    override suspend fun apply(req: IngestRequest): IngestRequest = req
}
