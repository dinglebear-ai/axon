package com.axon.app.data.remote

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()

private val json = Json {
    ignoreUnknownKeys = true
    coerceInputValues = true
}

class AxonClient(
    baseUrl: String,
    token: String,
) {
    // Thread-safe config: both baseUrl and token updated atomically together
    private val config = AtomicReference(baseUrl.trimEnd('/') to token)

    private val http = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(60, TimeUnit.SECONDS)
        .writeTimeout(15, TimeUnit.SECONDS)
        .build()

    // Research synthesis can take up to 2 minutes — built from the shared client to reuse connection pool
    private val httpLong = http.newBuilder()
        .readTimeout(120, TimeUnit.SECONDS)
        .build()

    fun updateConfig(newBaseUrl: String, newToken: String) {
        config.set(newBaseUrl.trimEnd('/') to newToken)
    }

    fun hasToken(): Boolean = config.get().second.isNotBlank()

    suspend fun healthz(): Boolean = withContext(Dispatchers.IO) {
        runCatching {
            val (baseUrl, _) = config.get()
            val req = Request.Builder()
                .url("$baseUrl/healthz")
                .get()
                .build()
            http.newCall(req).execute().use { it.isSuccessful }
        }
            .onFailure { if (it is CancellationException) throw it }
            .getOrDefault(false)
    }

    suspend fun ask(request: AskRequest): Result<AskResponse> = withContext(Dispatchers.IO) {
        post("/v1/ask", request)
    }

    suspend fun query(request: QueryRequest): Result<QueryResponse> = withContext(Dispatchers.IO) {
        post("/v1/query", request)
    }

    suspend fun sources(request: SourcesRequest = SourcesRequest()): Result<SourcesResponse> =
        withContext(Dispatchers.IO) {
            get("/v1/sources?limit=${request.limit}&offset=${request.offset}")
        }

    suspend fun stats(): Result<StatsResponse> = withContext(Dispatchers.IO) {
        get("/v1/stats")
    }

    suspend fun scrape(request: ScrapeRequest): Result<ScrapeResponse> = withContext(Dispatchers.IO) {
        post("/v1/scrape", request)
    }

    suspend fun map(request: MapRequest): Result<MapResponse> = withContext(Dispatchers.IO) {
        post("/v1/map", request)
    }

    suspend fun research(request: ResearchRequest): Result<ResearchResponse> = withContext(Dispatchers.IO) {
        postWith(httpLong, "/v1/research", request)
    }

    suspend fun crawlSubmit(request: CrawlRequest): Result<CrawlJobResponse> = withContext(Dispatchers.IO) {
        post("/v1/crawl", request)
    }

    suspend fun crawlStatus(jobId: String): Result<CrawlStatusResponse> = withContext(Dispatchers.IO) {
        get("/v1/crawl/$jobId")
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    private fun authRequest(builder: Request.Builder): Request.Builder {
        val (_, token) = config.get()
        return builder
            .header("Authorization", "Bearer $token")
            .header("x-api-key", token)
    }

    private fun baseUrl(): String = config.get().first

    private inline fun <reified B, reified R> post(path: String, body: B): Result<R> =
        postWith(http, path, body)

    private inline fun <reified B, reified R> postWith(client: OkHttpClient, path: String, body: B): Result<R> {
        val bodyBytes = json.encodeToString(body).toRequestBody(JSON_MEDIA_TYPE)
        val builder = authRequest(Request.Builder().url("${baseUrl()}$path").post(bodyBytes))
        return execute(client, builder)
    }

    private inline fun <reified R> get(path: String): Result<R> {
        val builder = authRequest(Request.Builder().url("${baseUrl()}$path").get())
        return execute(http, builder)
    }

    private inline fun <reified R> execute(client: OkHttpClient, builder: Request.Builder): Result<R> =
        runCatching {
            client.newCall(builder.build()).execute().use { resp ->
                if (!resp.isSuccessful) {
                    error("HTTP ${resp.code}: ${resp.body?.string() ?: resp.message}")
                }
                json.decodeFromString<R>(resp.body?.string() ?: error("Empty response body"))
            }
        }.onFailure { if (it is CancellationException) throw it }
}
