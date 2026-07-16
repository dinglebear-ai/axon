package com.axon.app.data.repository

import androidx.compose.runtime.Stable
import com.axon.app.core.api.AskRequest
import com.axon.app.core.api.AskStreamEvent
import com.axon.app.core.api.AxonClient
import com.axon.app.core.api.ChatRequest
import com.axon.app.core.api.MapRequest
import com.axon.app.core.api.QueryRequest
import com.axon.app.core.api.ResearchHit
import com.axon.app.core.api.ResearchRequest
import com.axon.app.core.api.RetrieveRequest
import com.axon.app.core.api.ScrapeRequest
import com.axon.app.core.api.SiteSourceRequest
import com.axon.app.core.api.SourcesRequest
import com.axon.app.core.api.artifactText
import com.axon.app.core.api.askStream
import com.axon.app.core.api.chatStream
import com.axon.app.core.api.deleteMobileSession
import com.axon.app.core.api.getMobileSession
import com.axon.app.core.api.listMobileSessions
import com.axon.app.core.api.models.EmbedRequest
import com.axon.app.core.api.models.ExtractRequest
import com.axon.app.core.api.models.MobileSessionDto
import com.axon.app.core.api.upsertMobileSession
import com.axon.app.data.local.AskHistoryDao
import com.axon.app.data.local.AskHistoryEntry
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive

@Stable data class AskResultUi(
    val query: String,
    val answer: String,
    val timingMs: Long?,
)

@Stable data class QueryHitUi(
    val rank: Long,
    val score: Double,
    val url: String,
    val source: String,
    val snippet: String,
)

@Stable data class SourceEntryUi(
    val url: String,
    val chunks: Int,
)

@Stable data class ScrapeResultUi(
    val url: String,
    val markdown: String,
)

@Stable data class RetrieveResultUi(
    val requestedUrl: String,
    val matchedUrl: String?,
    val chunkCount: Int,
    val content: String,
    val truncated: Boolean,
    val warnings: List<String>,
    val tokenEstimate: Int?,
    val nextCursor: String?,
    val remainingTokensEstimate: Int?,
    val refreshStatus: String?,
)

@Stable data class MapResultUi(
    val url: String,
    val total: Long,
    val urls: List<String>,
)

@Stable data class ResearchResultUi(
    val query: String,
    val summary: String?,
    val hits: List<ResearchHit>,
)

/** Source-job status including any server-reported error. */
@Stable data class SourceJobStatusUi(
    val jobId: String,
    val status: String,
    /** Non-null when the server reports a source-job failure reason. */
    val serverError: String?,
)

// ── Phase 2 UI models ─────────────────────────────────────────────────────────

@Stable data class SummarizeResultUi(
    val urls: List<String>,
    val summary: String,
    val contextChars: Long,
    val contextTruncated: Boolean,
)

@Stable data class SearchWebHitUi(
    val title: String,
    val url: String,
    val snippet: String?,
    val score: Double?,
)

@Stable data class SourceJobRefUi(
    val jobId: String,
    val url: String,
)

@Stable data class SearchWebResultUi(
    val query: String,
    val results: List<SearchWebHitUi>,
    val sourceJobsEnqueued: Int,
    val sourceJobsRejected: Int,
    val sourceJobs: List<SourceJobRefUi>,
)

@Stable data class JobUi(
    val kind: JobFamily? = null,
    val id: String,
    val status: String,
    val createdAt: String? = null,
    val startedAt: String? = null,
    val updatedAt: String? = null,
    val finishedAt: String? = null,
    val url: String?,
    val sourceKind: String?,
    val target: String?,
    val errorText: String?,
    val progressJson: kotlinx.serialization.json.JsonElement? = null,
    val resultJson: kotlinx.serialization.json.JsonElement?,
    val configJson: kotlinx.serialization.json.JsonElement? = null,
)

@Stable data class SuggestHitUi(
    val url: String,
    val reason: String?,
)

@Stable data class DomainFacetUi(
    val domain: String,
    val vectors: Long,
)

@Stable data class DomainIndexedUi(
    val domain: String,
    val indexed: Boolean,
)

@Stable data class WatchUi(
    val id: String,
    val name: String,
    val taskType: String,
    val enabled: Boolean,
    val everySeconds: Long,
    val nextRunAt: String?,
)

/** Mobile-safe first-page cap for `/v1/retrieve` calls. */
private const val DEFAULT_RETRIEVE_TOKEN_BUDGET = 10_000
private const val DEFAULT_RETRIEVE_MAX_POINTS = 48

class AxonRepository(
    private val client: AxonClient,
    private val askHistoryDao: AskHistoryDao,
    private val applicator: ModeOptionsApplicator,
) {
    // Short-circuits with a failure when no usable auth is configured; otherwise runs [block].
    private suspend inline fun <T> withAuth(block: () -> Result<T>): Result<T> =
        if (client.hasUsableAuth()) {
            block()
        } else {
            Result.failure(IllegalStateException("Axon is not authenticated. Go to Settings to sign in or add a bearer token."))
        }

    suspend fun ask(
        query: String,
        collection: String? = null,
    ): Result<AskResultUi> =
        withAuth {
            val req = applicator.apply(AskRequest(query = query, collection = collection))
            client.ask(req).map { r ->
                AskResultUi(query = r.query, answer = r.answer, timingMs = r.timingMs?.totalMs)
            }
        }

    /**
     * Streams the ask response via SSE. Emits [AskStreamEvent] objects as they arrive.
     * If no API token is configured, immediately emits a single [AskStreamEvent.Error].
     * Mode options are applied via [applicator] before the request is sent.
     */
    fun askStream(
        query: String,
        collection: String? = null,
    ): Flow<AskStreamEvent> =
        flow {
            if (!client.hasUsableAuth()) {
                emit(AskStreamEvent.Error("Axon is not authenticated. Go to Settings to sign in or add a bearer token."))
                return@flow
            }
            val req = applicator.apply(AskRequest(query = query, collection = collection))
            emitAll(client.askStream(req))
        }

    fun chatStream(message: String): Flow<AskStreamEvent> =
        flow {
            if (!client.hasUsableAuth()) {
                emit(AskStreamEvent.Error("Axon is not authenticated. Go to Settings to sign in or add a bearer token."))
                return@flow
            }
            emitAll(client.chatStream(ChatRequest(message = message)))
        }

    suspend fun query(
        query: String,
        limit: Int = 10,
        collection: String? = null,
    ): Result<List<QueryHitUi>> =
        withAuth {
            val req = applicator.apply(QueryRequest(query = query, limit = limit, collection = collection))
            client.query(req).map { r ->
                r.results.map { h ->
                    QueryHitUi(rank = h.rank, score = h.score, url = h.url, source = h.source, snippet = h.snippet)
                }
            }
        }

    /**
     * Fetch the full assembled document for [url].
     *
     * [tokenBudget] caps the server-side document window. Server signals
     * [RetrieveResultUi.truncated] when the cap is hit so the UI can show a banner.
     */
    suspend fun retrieve(
        url: String,
        collection: String? = null,
        tokenBudget: Int = DEFAULT_RETRIEVE_TOKEN_BUDGET,
        maxPoints: Int = DEFAULT_RETRIEVE_MAX_POINTS,
    ): Result<RetrieveResultUi> =
        withAuth {
            require(tokenBudget > 0) { "tokenBudget must be positive, got $tokenBudget" }
            require(maxPoints > 0) { "maxPoints must be positive, got $maxPoints" }
            client
                .retrieve(
                    RetrieveRequest(
                        url = url,
                        collection = collection,
                        maxPoints = maxPoints,
                        tokenBudget = tokenBudget,
                    ),
                ).map { r ->
                    RetrieveResultUi(
                        requestedUrl = r.requestedUrl ?: url,
                        matchedUrl = r.matchedUrl,
                        chunkCount = r.chunkCount,
                        content = r.content,
                        truncated = r.truncated,
                        warnings = r.warnings,
                        tokenEstimate = r.tokenEstimate,
                        nextCursor = r.nextCursor,
                        remainingTokensEstimate = r.remainingTokensEstimate,
                        refreshStatus = r.refreshStatus,
                    )
                }
        }

    suspend fun sources(
        limit: Int = 50,
        offset: Int = 0,
        domain: String? = null,
        cursor: String? = null,
    ): Result<List<SourceEntryUi>> =
        withAuth {
            client.sources(SourcesRequest(limit = limit, offset = offset, domain = domain, cursor = cursor)).mapCatching { r ->
                var parseFailures = 0
                val entries =
                    r.urls.mapNotNull { element ->
                        runCatching {
                            val arr = element.jsonArray
                            if (arr.size < 2) return@mapNotNull null
                            SourceEntryUi(
                                url = arr[0].jsonPrimitive.content,
                                chunks = arr[1].jsonPrimitive.int,
                            )
                        }.getOrElse {
                            parseFailures++
                            null
                        }
                    }
                // Surface a failure if the server returned entries but we decoded zero —
                // this indicates an API contract change (e.g. shape changed from [[url, n]] to [{url, n}]).
                if (entries.isEmpty() && r.urls.isNotEmpty()) {
                    error(
                        "Failed to parse ${r.urls.size} source entries from the server response. " +
                            "The server may be returning an unexpected format.",
                    )
                }
                // Non-fatal: some entries failed to parse, but at least some succeeded.
                // The caller sees partial results; a future improvement could expose `parseFailures`.
                entries
            }
        }

    suspend fun scrape(url: String): Result<ScrapeResultUi> =
        withAuth {
            val req = applicator.apply(ScrapeRequest(url = url))
            client.scrape(req).map { r ->
                ScrapeResultUi(url = r.url, markdown = r.markdown)
            }
        }

    suspend fun map(url: String): Result<MapResultUi> =
        withAuth {
            val req = applicator.apply(MapRequest(url = url))
            client.map(req).map { r ->
                MapResultUi(url = r.url, total = r.total, urls = r.urls)
            }
        }

    suspend fun research(query: String): Result<ResearchResultUi> =
        withAuth {
            val req = applicator.apply(ResearchRequest(query = query))
            client.research(req).map { r ->
                ResearchResultUi(
                    query = r.payload.query,
                    summary = r.payload.summary,
                    hits = r.payload.searchResults,
                )
            }
        }

    suspend fun sourceSiteSubmit(
        url: String,
        maxPages: Int? = null,
    ): Result<String> = sourceSiteSubmit(url, SiteSourceSubmitOptions(maxPages = maxPages))

    suspend fun sourceSiteSubmit(
        url: String,
        options: SiteSourceSubmitOptions,
    ): Result<String> =
        withAuth {
            val req = applicator.apply(options.requestFor(url))
            client.sourceSiteSubmit(req).map { it.jobId }
        }

    suspend fun sourceJobStatus(jobId: String): Result<SourceJobStatusUi> =
        withAuth {
            client.sourceJobStatus(jobId).map { r ->
                SourceJobStatusUi(
                    jobId = r.jobId.ifBlank { jobId },
                    status = r.status.ifBlank { "unknown" },
                    serverError = r.lastError?.toString(),
                )
            }
        }

    suspend fun ping(): Boolean = client.healthz().isSuccess

    // ── Ask history ───────────────────────────────────────────────────────────

    /**
     * Persists an ask history entry. Returns true on success, false if the Room
     * insert fails (e.g. disk full). The failure is non-fatal — the ask result
     * was already shown — but callers should log a warning rather than silently
     * swallowing it.
     */
    suspend fun recordAskHistory(entry: AskHistoryEntry): Boolean =
        runCatching {
            askHistoryDao.insert(entry)
            true
        }.getOrDefault(false)

    fun recentHistory(): Flow<List<AskHistoryEntry>> = askHistoryDao.recent()

    suspend fun listMobileSessions(): Result<List<MobileSessionDto>> =
        withAuth {
            client.listMobileSessions()
        }

    suspend fun getMobileSession(id: String): Result<MobileSessionDto> =
        withAuth {
            client.getMobileSession(id)
        }

    suspend fun upsertMobileSession(session: MobileSessionDto): Result<MobileSessionDto> =
        withAuth {
            client.upsertMobileSession(session)
        }

    suspend fun deleteMobileSession(id: String): Result<Boolean> =
        withAuth {
            client.deleteMobileSession(id)
        }

    // ── Phase 2 wrappers ───────────────────────────────────────────────────

    suspend fun summarize(
        urls: List<String>,
        collection: String? = null,
    ): Result<SummarizeResultUi> =
        withAuth {
            val req =
                applicator.apply(
                    com.axon.app.core.api.models
                        .SummarizeRequest(urls = urls, collection = collection),
                )
            client.summarize(req).map { r -> SummarizeResultUi(r.urls, r.summary, r.contextChars, r.contextTruncated) }
        }

    suspend fun searchWeb(query: String): Result<SearchWebResultUi> =
        withAuth {
            val req =
                applicator.apply(
                    com.axon.app.core.api.models
                        .SearchWebRequest(query = query),
                )
            client.searchWeb(req).map { r ->
                SearchWebResultUi(
                    query = r.query,
                    results = r.results.map { SearchWebHitUi(it.title, it.url, it.snippet, it.score) },
                    sourceJobsEnqueued = r.sourceJobs.size,
                    sourceJobsRejected = r.sourceJobsRejected.size,
                    sourceJobs = r.sourceJobs.map { SourceJobRefUi(it.jobId, it.url) },
                )
            }
        }

    suspend fun sourceSubmit(
        target: String,
        options: SourceSubmitOptions = SourceSubmitOptions(),
    ): Result<String> =
        withAuth {
            val req = applicator.apply(options.requestFor(target = target))
            client.sourceSubmit(req).map { result -> result.job?.id?.ifBlank { null } ?: result.jobId }
        }

    suspend fun extractStart(
        url: String,
        prompt: String? = null,
    ): Result<String> =
        withAuth {
            client.extractStart(ExtractRequest(urls = listOf(url), prompt = prompt?.takeIf { it.isNotBlank() })).map { it.jobId }
        }

    suspend fun embedStart(
        input: String,
        collection: String? = null,
    ): Result<String> =
        withAuth {
            client.embedStart(EmbedRequest(input = input, collection = collection)).map { it.jobId }
        }

    suspend fun getJob(
        kind: JobFamily,
        id: String,
    ): Result<JobUi> =
        withAuth {
            val clientKind = kind.toClientKind()
            client.getJob(clientKind, id).map { it.toJobUi(kind) }
        }

    suspend fun listJobs(kind: JobFamily): Result<List<JobUi>> =
        withAuth {
            val clientKind = kind.toClientKind()
            client.listJobs(clientKind).map { list -> list.map { it.toJobUi(kind) } }
        }

    suspend fun listWatches(): Result<List<WatchUi>> =
        withAuth {
            client.listWatches().map { watches ->
                watches.map {
                    WatchUi(
                        id = it.displayId,
                        name = it.displayName,
                        taskType = it.displayTaskType,
                        enabled = it.enabled,
                        everySeconds = it.everySeconds,
                        nextRunAt = it.nextRunAt,
                    )
                }
            }
        }

    suspend fun artifactText(relativePath: String): Result<String> =
        withAuth {
            client.artifactText(relativePath)
        }

    suspend fun cancelJob(
        kind: JobFamily,
        id: String,
    ): Result<Boolean> =
        withAuth {
            client.cancelJob(kind.toClientKind(), id).map { it.canceled }
        }

    suspend fun statusPayload(): Result<kotlinx.serialization.json.JsonElement> =
        withAuth {
            client.status().map { it.payload }
        }

    suspend fun statsPayload(): Result<kotlinx.serialization.json.JsonElement> =
        withAuth {
            client.stats().map { it.payload }
        }

    suspend fun doctorPayload(): Result<kotlinx.serialization.json.JsonElement> =
        withAuth {
            client.doctor().map { it.payload }
        }

    suspend fun suggest(
        focus: String?,
        collection: String? = null,
    ): Result<List<SuggestHitUi>> =
        withAuth {
            client.suggest(focus = focus, collection = collection).map { r ->
                val hits = r.suggestions.ifEmpty { r.urls }
                hits.map { SuggestHitUi(it.url, it.reason) }
            }
        }

    suspend fun domains(
        limit: Int = 100,
        offset: Int = 0,
    ): Result<List<DomainFacetUi>> =
        withAuth {
            client.domains(limit = limit, offset = offset).map { r ->
                r.domains.map { DomainFacetUi(it.domain, it.vectors) }
            }
        }

    suspend fun domainIndexed(domain: String): Result<DomainIndexedUi> =
        withAuth {
            client.domainIndexed(domain).map { r -> DomainIndexedUi(r.domain, r.indexed) }
        }
}
