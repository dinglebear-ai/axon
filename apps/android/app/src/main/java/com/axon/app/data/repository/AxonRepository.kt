package com.axon.app.data.repository

import androidx.compose.runtime.Stable
import com.axon.app.data.local.AskHistoryDao
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.remote.AskRequest
import com.axon.app.data.remote.AskStreamEvent
import com.axon.app.data.remote.CrawlRequest
import com.axon.app.data.remote.MapRequest
import com.axon.app.data.remote.QueryRequest
import com.axon.app.data.remote.ResearchRequest
import com.axon.app.data.remote.RetrieveRequest
import com.axon.app.data.remote.ScrapeRequest
import com.axon.app.data.remote.SourcesRequest
import com.axon.app.data.remote.ResearchHit
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.int

@Stable data class AskResultUi(val query: String, val answer: String, val timingMs: Long?)
@Stable data class QueryHitUi(val rank: Long, val score: Double, val url: String, val source: String, val snippet: String)
@Stable data class SourceEntryUi(val url: String, val chunks: Int)
@Stable data class ScrapeResultUi(val url: String, val markdown: String)
@Stable data class RetrieveResultUi(
    val requestedUrl: String,
    val matchedUrl: String?,
    val chunkCount: Int,
    val content: String,
    val truncated: Boolean,
    val warnings: List<String>,
)
@Stable data class MapResultUi(val url: String, val total: Long, val urls: List<String>)
@Stable data class ResearchResultUi(val query: String, val summary: String?, val hits: List<ResearchHit>)
/** Full crawl status including server-reported error and page count so callers can show actionable feedback. */
@Stable data class CrawlStatusUi(
    val jobId: String,
    val status: String,
    val pagesCrawled: Int?,
    /** Non-null when the server reports a crawl failure reason. */
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
    val title: String, val url: String, val snippet: String?, val score: Double?,
)
@Stable data class CrawlJobRefUi(val jobId: String, val url: String)
@Stable data class SearchWebResultUi(
    val query: String,
    val results: List<SearchWebHitUi>,
    val crawlJobsEnqueued: Int,
    val crawlJobsSkipped: Int,
    val crawlJobs: List<CrawlJobRefUi>,
)

@Stable data class JobUi(
    val id: String,
    val status: String,
    val url: String?,
    val sourceType: String?,
    val target: String?,
    val errorText: String?,
    val resultJson: kotlinx.serialization.json.JsonElement?,
    val finishedAt: String?,
)

@Stable data class SuggestHitUi(val url: String, val reason: String?)
@Stable data class DomainFacetUi(val domain: String, val vectors: Long)

/** Default `token_budget` cap for `/v1/retrieve` calls. */
private const val DEFAULT_RETRIEVE_TOKEN_BUDGET = 64_000

class AxonRepository(
    private val client: AxonClient,
    private val askHistoryDao: AskHistoryDao,
    private val applicator: ModeOptionsApplicator,
) {

    // Short-circuits with a failure when no token is configured; otherwise runs [block].
    private suspend inline fun <T> withToken(block: () -> Result<T>): Result<T> =
        if (client.hasToken()) block()
        else Result.failure(IllegalStateException("No API token configured. Go to Settings to add your token."))

    suspend fun ask(query: String, collection: String? = null): Result<AskResultUi> = withToken {
        val req = applicator.apply(AskRequest(query = query, collection = collection))
        client.ask(req).map { r ->
            AskResultUi(query = r.query, answer = r.answer, timingMs = r.timingMs?.totalMs)
        }
    }

    /**
     * Streams the ask response via SSE. Emits [AskStreamEvent] objects as they arrive.
     * If no API token is configured, immediately emits a single [AskStreamEvent.Error].
     *
     * Note: applicator merge can't easily run inside a non-suspending `flow {}` builder
     * because the merge itself is suspending. We construct the base request synchronously;
     * persisted overrides will land on the next non-streaming call. SSE streaming is
     * intentionally narrow surface area — extending mode-options here is a follow-up.
     */
    fun askStream(query: String, collection: String? = null): Flow<AskStreamEvent> {
        if (!client.hasToken()) return flow {
            emit(AskStreamEvent.Error("No API token configured. Go to Settings to add your token."))
        }
        return client.askStream(AskRequest(query = query, collection = collection))
    }

    suspend fun query(query: String, limit: Int = 10, collection: String? = null): Result<List<QueryHitUi>> = withToken {
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
    ): Result<RetrieveResultUi> = withToken {
        client.retrieve(
            RetrieveRequest(url = url, collection = collection, tokenBudget = tokenBudget),
        ).map { r ->
            RetrieveResultUi(
                requestedUrl = r.requestedUrl ?: url,
                matchedUrl = r.matchedUrl,
                chunkCount = r.chunkCount,
                content = r.content,
                truncated = r.truncated,
                warnings = r.warnings,
            )
        }
    }

    suspend fun sources(limit: Int = 50, offset: Int = 0, collection: String? = null): Result<List<SourceEntryUi>> = withToken {
        client.sources(SourcesRequest(limit = limit, offset = offset, collection = collection)).mapCatching { r ->
            var parseFailures = 0
            val entries = r.urls.mapNotNull { element ->
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
                    "The server may be returning an unexpected format."
                )
            }
            // Non-fatal: some entries failed to parse, but at least some succeeded.
            // The caller sees partial results; a future improvement could expose `parseFailures`.
            entries
        }
    }

    suspend fun scrape(url: String): Result<ScrapeResultUi> = withToken {
        val req = applicator.apply(ScrapeRequest(url = url))
        client.scrape(req).map { r ->
            ScrapeResultUi(url = r.url, markdown = r.markdown)
        }
    }

    suspend fun map(url: String): Result<MapResultUi> = withToken {
        val req = applicator.apply(MapRequest(url = url))
        client.map(req).map { r ->
            MapResultUi(url = r.url, total = r.total, urls = r.urls)
        }
    }

    suspend fun research(query: String): Result<ResearchResultUi> = withToken {
        val req = applicator.apply(ResearchRequest(query = query))
        client.research(req).map { r ->
            ResearchResultUi(
                query = r.payload.query,
                summary = r.payload.summary,
                hits = r.payload.searchResults,
            )
        }
    }

    suspend fun crawlSubmit(url: String, maxPages: Int? = null): Result<String> = withToken {
        val req = applicator.apply(CrawlRequest(urls = listOf(url), maxPages = maxPages))
        client.crawlSubmit(req).map { it.jobId }
    }

    suspend fun crawlStatus(jobId: String): Result<CrawlStatusUi> = withToken {
        client.crawlStatus(jobId).map { r ->
            CrawlStatusUi(
                jobId = r.jobId.ifBlank { jobId },
                status = r.status.ifBlank { "unknown" },
                pagesCrawled = r.pagesCrawled,
                serverError = r.error,
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
        runCatching { askHistoryDao.insert(entry); true }
            .getOrDefault(false)

    fun recentHistory(): Flow<List<AskHistoryEntry>> = askHistoryDao.recent()

    // ── Phase 2 wrappers ───────────────────────────────────────────────────

    suspend fun summarize(urls: List<String>, collection: String? = null): Result<SummarizeResultUi> = withToken {
        val req = applicator.apply(
            com.axon.app.data.remote.models.SummarizeRequest(urls = urls, collection = collection)
        )
        client.summarize(req).map { r -> SummarizeResultUi(r.urls, r.summary, r.contextChars, r.contextTruncated) }
    }

    suspend fun searchWeb(query: String): Result<SearchWebResultUi> = withToken {
        val req = applicator.apply(com.axon.app.data.remote.models.SearchWebRequest(query = query))
        client.searchWeb(req).map { r ->
            SearchWebResultUi(
                query = r.query,
                results = r.results.map { SearchWebHitUi(it.title, it.url, it.snippet, it.score) },
                crawlJobsEnqueued = r.autoCrawlStatus?.enqueued ?: 0,
                crawlJobsSkipped = r.autoCrawlStatus?.skipped ?: 0,
                crawlJobs = r.crawlJobs.map { CrawlJobRefUi(it.jobId, it.url) },
            )
        }
    }

    suspend fun ingestStart(sourceType: String, target: String, collection: String? = null): Result<String> = withToken {
        val req = applicator.apply(
            com.axon.app.data.remote.models.IngestRequest(sourceType = sourceType, target = target, collection = collection)
        )
        client.ingestStart(req).map { it.jobId }
    }

    suspend fun getJob(kind: AxonClient.JobKind, id: String): Result<JobUi> = withToken {
        client.getJob(kind, id).map(::toJobUi)
    }

    suspend fun listJobs(kind: AxonClient.JobKind): Result<List<JobUi>> = withToken {
        client.listJobs(kind).map { list -> list.map(::toJobUi) }
    }

    suspend fun cancelJob(kind: AxonClient.JobKind, id: String): Result<Boolean> = withToken {
        client.cancelJob(kind, id).map { it.canceled }
    }

    suspend fun statusPayload(): Result<kotlinx.serialization.json.JsonElement> = withToken {
        client.status().map { it.payload }
    }

    suspend fun statsPayload(): Result<kotlinx.serialization.json.JsonElement> = withToken {
        client.stats().map { it.payload }
    }

    suspend fun doctorPayload(): Result<kotlinx.serialization.json.JsonElement> = withToken {
        client.doctor().map { it.payload }
    }

    suspend fun suggest(focus: String?, collection: String? = null): Result<List<SuggestHitUi>> = withToken {
        client.suggest(focus = focus, collection = collection).map { r ->
            r.urls.map { SuggestHitUi(it.url, it.reason) }
        }
    }

    suspend fun domains(limit: Int = 100, offset: Int = 0): Result<List<DomainFacetUi>> = withToken {
        client.domains(limit = limit, offset = offset).map { r ->
            r.domains.map { DomainFacetUi(it.domain, it.vectors) }
        }
    }

    private fun toJobUi(j: com.axon.app.data.remote.models.ServiceJob) = JobUi(
        id = j.id, status = j.status, url = j.url, sourceType = j.sourceType,
        target = j.target, errorText = j.errorText, resultJson = j.resultJson,
        finishedAt = j.finishedAt,
    )
}
