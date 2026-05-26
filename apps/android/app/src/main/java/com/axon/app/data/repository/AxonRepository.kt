package com.axon.app.data.repository

import com.axon.app.data.remote.AxonClient
import com.axon.app.data.remote.AskRequest
import com.axon.app.data.remote.QueryRequest
import com.axon.app.data.remote.SourcesRequest
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.int

data class AskResultUi(val query: String, val answer: String, val timingMs: Long?)
data class QueryHitUi(val rank: Long, val score: Double, val url: String, val source: String, val snippet: String)
data class SourceEntryUi(val url: String, val chunks: Int)

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

    suspend fun ping(): Boolean = client.healthz()
}
