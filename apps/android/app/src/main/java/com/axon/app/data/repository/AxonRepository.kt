package com.axon.app.data.repository

import androidx.compose.runtime.Stable
import com.axon.app.data.local.AskHistoryDao
import com.axon.app.data.local.AskHistoryEntry
import com.axon.app.data.remote.AxonClient
import com.axon.app.data.remote.AskRequest
import com.axon.app.data.remote.CrawlRequest
import com.axon.app.data.remote.MapRequest
import com.axon.app.data.remote.QueryRequest
import com.axon.app.data.remote.ResearchRequest
import com.axon.app.data.remote.ScrapeRequest
import com.axon.app.data.remote.SourcesRequest
import com.axon.app.data.remote.ResearchHit
import kotlinx.coroutines.flow.Flow
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.int

@Stable data class AskResultUi(val query: String, val answer: String, val timingMs: Long?)
@Stable data class QueryHitUi(val rank: Long, val score: Double, val url: String, val source: String, val snippet: String)
@Stable data class SourceEntryUi(val url: String, val chunks: Int)
@Stable data class ScrapeResultUi(val url: String, val markdown: String)
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

class AxonRepository(
    private val client: AxonClient,
    private val askHistoryDao: AskHistoryDao,
) {

    // Short-circuits with a failure when no token is configured; otherwise runs [block].
    private suspend inline fun <T> withToken(block: () -> Result<T>): Result<T> =
        if (client.hasToken()) block()
        else Result.failure(IllegalStateException("No API token configured. Go to Settings to add your token."))

    suspend fun ask(query: String, collection: String? = null): Result<AskResultUi> = withToken {
        client.ask(AskRequest(query = query, collection = collection)).map { r ->
            AskResultUi(query = r.query, answer = r.answer, timingMs = r.timingMs?.totalMs)
        }
    }

    suspend fun query(query: String, limit: Int = 10, collection: String? = null): Result<List<QueryHitUi>> = withToken {
        client.query(QueryRequest(query = query, limit = limit, collection = collection)).map { r ->
            r.results.map { h ->
                QueryHitUi(rank = h.rank, score = h.score, url = h.url, source = h.source, snippet = h.snippet)
            }
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

    suspend fun scrape(url: String, collection: String? = null): Result<ScrapeResultUi> = withToken {
        client.scrape(ScrapeRequest(url = url, collection = collection)).map { r ->
            ScrapeResultUi(url = r.url, markdown = r.markdown)
        }
    }

    suspend fun map(url: String): Result<MapResultUi> = withToken {
        client.map(MapRequest(url = url)).map { r ->
            MapResultUi(url = r.url, total = r.total, urls = r.urls)
        }
    }

    suspend fun research(query: String): Result<ResearchResultUi> = withToken {
        client.research(ResearchRequest(query = query)).map { r ->
            ResearchResultUi(
                query = r.payload.query,
                summary = r.payload.summary,
                hits = r.payload.searchResults,
            )
        }
    }

    suspend fun crawlSubmit(url: String, maxPages: Int? = null, collection: String? = null): Result<String> = withToken {
        client.crawlSubmit(CrawlRequest(urls = listOf(url), maxPages = maxPages, collection = collection)).map { it.jobId }
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
}
