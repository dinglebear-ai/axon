package com.axon.app.data.repository

import com.axon.app.data.remote.AxonClient
import com.axon.app.data.remote.AskRequest
import com.axon.app.data.remote.CrawlRequest
import com.axon.app.data.remote.MapRequest
import com.axon.app.data.remote.QueryRequest
import com.axon.app.data.remote.ResearchRequest
import com.axon.app.data.remote.ScrapeRequest
import com.axon.app.data.remote.SourcesRequest
import com.axon.app.data.remote.ResearchHit
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.int

data class AskResultUi(val query: String, val answer: String, val timingMs: Long?)
data class QueryHitUi(val rank: Long, val score: Double, val url: String, val source: String, val snippet: String)
data class SourceEntryUi(val url: String, val chunks: Int)
data class ScrapeResultUi(val url: String, val markdown: String)
data class MapResultUi(val url: String, val total: Long, val urls: List<String>)
data class ResearchResultUi(val query: String, val summary: String?, val hits: List<ResearchHit>)

class AxonRepository(private val client: AxonClient) {

    suspend fun ask(query: String, collection: String? = null): Result<AskResultUi> =
        client.ask(AskRequest(query = query, collection = collection)).map { r ->
            AskResultUi(query = r.query, answer = r.answer, timingMs = r.timingMs?.totalMs)
        }

    suspend fun query(query: String, limit: Int = 10, collection: String? = null): Result<List<QueryHitUi>> =
        client.query(QueryRequest(query = query, limit = limit, collection = collection)).map { r ->
            r.results.map { h ->
                QueryHitUi(rank = h.rank, score = h.score, url = h.url, source = h.source, snippet = h.snippet)
            }
        }

    suspend fun sources(limit: Int = 50, offset: Int = 0): Result<List<SourceEntryUi>> =
        client.sources(SourcesRequest(limit = limit, offset = offset)).map { r ->
            r.urls.mapNotNull { element ->
                runCatching {
                    val arr = element.jsonArray
                    SourceEntryUi(
                        url = arr[0].jsonPrimitive.content,
                        chunks = arr[1].jsonPrimitive.int,
                    )
                }.getOrNull()
            }
        }

    suspend fun scrape(url: String): Result<ScrapeResultUi> =
        client.scrape(ScrapeRequest(url = url)).map { r ->
            ScrapeResultUi(url = r.url, markdown = r.markdown)
        }

    suspend fun map(url: String): Result<MapResultUi> =
        client.map(MapRequest(url = url)).map { r ->
            MapResultUi(url = r.url, total = r.total, urls = r.urls)
        }

    suspend fun research(query: String): Result<ResearchResultUi> =
        client.research(ResearchRequest(query = query)).map { r ->
            ResearchResultUi(
                query = r.payload.query,
                summary = r.payload.summary,
                hits = r.payload.search_results,
            )
        }

    suspend fun crawlSubmit(url: String, maxPages: Int? = null): Result<String> =
        client.crawlSubmit(CrawlRequest(urls = listOf(url), max_pages = maxPages)).map { it.job_id }

    suspend fun crawlStatus(jobId: String): Result<String> =
        client.crawlStatus(jobId).map { obj ->
            obj["status"]?.jsonPrimitive?.content ?: "unknown"
        }

    suspend fun ping(): Boolean = client.healthz()
}
